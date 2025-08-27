use crate::{Config, ControlMessage, Line, Result, RunState, Wordlist};
use color_eyre::eyre::eyre;
use std::{
    path::Path, process::Stdio, str::FromStr, sync::Arc, time::Duration,
};
use tokio::{
    io::{AsyncReadExt, BufReader},
    sync::mpsc,
};
use tokio_stream::{wrappers::ReceiverStream, Stream, StreamExt};

const TEST_LINES: &str = include_str!("test-data.txt");

pub struct Auth {
    pub region: String,
    pub key: String,
}

struct SetupState {
    language: Arc<str>,
    wordlist: Option<Arc<str>>,
}

impl Default for SetupState {
    fn default() -> Self {
        Self {
            language: crate::LANGUAGE_OPTIONS[0].into(),
            wordlist: None,
        }
    }
}

// spx recognize --microphone --phrases @/tmp/words.txt --language en-GB

pub fn start(
    tx: mpsc::Sender<Line>,
    control_rx: mpsc::Receiver<ControlMessage>,
    auth: Auth,
    config: Config,
) {
    tokio::task::spawn(async move {
        start_inner(tx, control_rx, auth, config).await.unwrap()
    });
}

// State machine:
// - Stopped: wait for control channel message to transition to other state
// - Running: start azure client and then select! on that and the control channel
// - Test: start test loop and then select! on that and the control channel
async fn start_inner(
    tx: mpsc::Sender<Line>,
    mut control_rx: mpsc::Receiver<ControlMessage>,
    auth: Auth,
    config: Config,
) -> Result<()> {
    let mut run_state = RunState::Stopped;
    let mut setup_state = SetupState::default();

    let azure_auth =
        azure_speech::Auth::from_subscription(auth.region, auth.key);

    loop {
        run_state = match run_state {
            RunState::Stopped => {
                wait_for_transition(&mut control_rx, &mut setup_state, &config)
                    .await
            }
            RunState::Running => {
                match do_run(
                    &tx,
                    &mut control_rx,
                    &mut setup_state,
                    &azure_auth,
                    &config,
                )
                .await
                {
                    Ok(state) => state,
                    Err(err) => {
                        error!("{err}");
                        RunState::Stopped
                    }
                }
            }
            RunState::Test => {
                run_test(&tx, &mut control_rx, &mut setup_state, &config).await
            }
        };
    }
}

async fn wait_for_transition(
    control_rx: &mut mpsc::Receiver<ControlMessage>,
    setup_state: &mut SetupState,
    config: &Config,
) -> RunState {
    while let Some(msg) = control_rx.recv().await {
        match msg {
            ControlMessage::SetState(new_state) => return new_state,
            other => handle_lang_and_wordlist(other, setup_state, config),
        }
    }
    RunState::Stopped
}

fn langauge_from_language(lang: &str) -> azure_speech::recognizer::Language {
    match lang {
        "en-GB" => azure_speech::recognizer::Language::EnGb,
        "en-IE" => azure_speech::recognizer::Language::EnIe,
        "en-US" => azure_speech::recognizer::Language::EnUs,
        "ja-JP" => azure_speech::recognizer::Language::JaJp,
        _ => azure_speech::recognizer::Language::EnGb,
    }
}

async fn do_run(
    tx: &mpsc::Sender<Line>,
    control_rx: &mut mpsc::Receiver<ControlMessage>,
    setup_state: &mut SetupState,
    auth: &azure_speech::Auth,
    config: &Config,
) -> Result<RunState> {
    let mut azure_config = azure_speech::recognizer::Config::default()
        .set_language(langauge_from_language(&setup_state.language));

    if let (Some(wordlist_dir), Some(wordlist_file)) =
        (&config.wordlist_dir, &setup_state.wordlist)
    {
        let wordlist_path = wordlist_dir.join(wordlist_file.as_ref());
        let wordlist = std::fs::read_to_string(wordlist_path)?;
        let wordlist = wordlist
            .lines()
            .filter(|line| !line.is_empty())
            .map(String::from)
            .collect();
        azure_config = azure_config.set_phrases(wordlist);
    }

    let client =
        azure_speech::recognizer::Client::connect(auth.clone(), azure_config)
            .await
            .map_err(|err| eyre!("{err:?}"))?;

    let stream = listen_from_default_input().await?;

    let mut events = client
        .recognize(
            stream,
            azure_speech::recognizer::AudioFormat::WebmOpus,
            azure_speech::recognizer::AudioDevice::new(
                azure_speech::recognizer::SourceType::Microphones,
            ),
        )
        .await
        .map_err(|err| eyre!("{err:?}"))?;

    tracing::info!("... Starting to listen from microphone ...");

    loop {
        tokio::select! {
            event = events.next() => {
                let Some(event) = event else { break };
                dbg!(&event);
                use azure_speech::recognizer::Event;
                let  line =

                match event {
                    Ok(Event::Recognized(_, result, _, _, _)) => {
                        Some(Line::Recognised(result.text.clone()))
                    }
                    Ok(Event::Recognizing(_, result, _, _, _)) => {
                        Some(Line::Recognising(result.text.clone()))
                    }
                    Err(err) => {
                        error!("{err:?}");
                        None
                    }
                    _ => None,
                };

                if let Some(line) = line &&
                     tx.try_send(line).is_err() {
                        warn!("Line channel full");

                }
            }
            msg = control_rx.recv() => {
                let Some(msg) = msg else { break };
                match msg {
                    ControlMessage::SetState(new_state) => {
                        if new_state != RunState::Running {
                            info!("Shutting down azure speech client");
                            if let Err(err) = client.disconnect().await{
                                error!("{err:?}");
                            }
                            return Ok(new_state);
                        }
                    }
                                       other => {
                        handle_lang_and_wordlist(other, setup_state,config);
                    }
                }

            }
        }
    }

    Ok(RunState::Stopped)
}

// ffmpeg -y -f pulse -ac 2 -i default -f webm /dev/stdout
async fn listen_from_default_input() -> Result<impl Stream<Item = Vec<u8>>> {
    let (tx, rx) = mpsc::channel(10);

    let mut child = tokio::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "pulse",
            "-ac",
            "2",
            "-i",
            "default",
            "-f",
            "webm",
            "/dev/stdout",
        ])
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().unwrap();

    tokio::task::spawn(async move {
        child.wait().await.unwrap();
    });

    tokio::task::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut buf = [0; 1024];
        let mut errors = 0_usize;
        loop {
            match reader.read_exact(&mut buf).await {
                Ok(_) => errors = 0,
                Err(err) => {
                    warn!("{err}");
                    errors += 1;
                    if errors > 5 {
                        error!(
                            "Max errors reached, unable to read ffmpeg stream"
                        );
                        break;
                    }
                    continue;
                }
            }
            if tx.send(buf.to_vec()).await.is_err() {
                info!("Stream closed");
                break;
            }
        }
    });

    Ok(ReceiverStream::new(rx))
}

async fn run_test(
    tx: &mpsc::Sender<Line>,
    control_rx: &mut mpsc::Receiver<ControlMessage>,
    setup_state: &mut SetupState,
    config: &Config,
) -> RunState {
    const LINE_DELAY: Duration = Duration::from_millis(300);

    let mut interval = tokio::time::interval(LINE_DELAY);
    let lines = TEST_LINES
        .lines()
        .filter(|line| !line.is_empty())
        .map(Line::from_str)
        .collect::<Result<Vec<_>>>()
        .unwrap();
    let mut lines_iter = lines.iter().cycle();

    loop {
        tokio::select! {
            _ = interval.tick() => {
                tx.send(lines_iter.next().unwrap().clone()).await.unwrap();
            }
            msg = control_rx.recv() => {
                let Some(msg) = msg else {break RunState::Stopped};
                match msg {
                    ControlMessage::SetState(new_state) => {
                        if new_state != RunState::Test {
                            break new_state;
                        }
                    }
                                       other => {
                        handle_lang_and_wordlist(other, setup_state,config);
                    }
                }

            }
        }
    }
}

fn handle_lang_and_wordlist(
    msg: ControlMessage,
    setup_state: &mut SetupState,
    config: &Config,
) {
    match msg {
        ControlMessage::GetWordlist(reply) => {
            let options = config
                .wordlist_dir
                .as_deref()
                .map(list_wordlists)
                .unwrap_or_default();
            let _ = reply.send(Wordlist {
                options,
                current: setup_state.wordlist.clone(),
            });
        }
        ControlMessage::SetWordlist(choice) => {
            let options = config
                .wordlist_dir
                .as_deref()
                .map(list_wordlists)
                .unwrap_or_default();
            if let Some(choice) = choice {
                if options.contains(&choice) {
                    setup_state.wordlist = Some(choice);
                } else {
                    warn!("Invalid wordlist choice `{choice:?}`");
                }
            } else {
                setup_state.wordlist = None;
            }
        }
        other => panic!("Unreachable: {other:?}"),
    }
}

fn list_wordlists(dir: &Path) -> Vec<Arc<str>> {
    let mut options = Vec::new();

    for entry in dir.read_dir().unwrap() {
        let Ok(entry) = entry else { continue };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_file() {
            let Ok(file_name) = entry.file_name().into_string() else {
                continue;
            };
            options.push(file_name.into());
        }
    }

    options
}
