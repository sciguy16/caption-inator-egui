use crate::{
    ControlMessage, Line, Result, RunState, config::Config,
    listener::SetupState,
};
use color_eyre::eyre::eyre;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

// spx recognize --microphone --phrases @/tmp/words.txt --language en-GB
pub async fn do_run(
    tx: &mpsc::Sender<Line>,
    control_rx: &mut mpsc::Receiver<ControlMessage>,
    setup_state: &mut SetupState,
    auth: &azure_speech::Auth,
    config: &Config,
) -> Result<RunState> {
    let mut azure_config = azure_speech::recognizer::Config::default()
        .set_language(langauge_from_language(&setup_state.language))
        .set_profanity(azure_speech::recognizer::Profanity::Raw);

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

    'reconnection: loop {
        let client = azure_speech::recognizer::Client::connect(
            auth.clone(),
            azure_config.clone(),
        )
        .await
        .map_err(|err| eyre!("{err:?}"))?;

        let stream = super::listen_from_default_input("webm").await?;

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

        let new_state = loop {
            tokio::select! {
                event = events.next() => {
                    if let Err(err) = handle_event(event, tx).await {
                        error!("{err:?}");
                        break RunState::Running;
                    }
                }
                msg = control_rx.recv() => {
                    let Some(msg) = msg else { break 'reconnection};
                    match msg {
                        ControlMessage::SetState(RunState::Running) => {}
                        ControlMessage::SetState(new_state) => {
                           break new_state;
                        }
                        other => {
                         super::   handle_lang_and_wordlist(
                                other, setup_state, config,
                            );
                        }
                    }

                }
            }
        };

        if let Err(err) = client.disconnect().await {
            warn!("Disconnection failed: {err}");
        }

        if new_state != RunState::Running {
            info!("Azure speech client shut down");
            return Ok(new_state);
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
        info!("Reconnecting");
    }

    Ok(RunState::Stopped)
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

async fn handle_event(
    event: Option<Result<azure_speech::recognizer::Event, azure_speech::Error>>,
    tx: &mpsc::Sender<Line>,
) -> Result<(), azure_speech::Error> {
    use azure_speech::recognizer::Event;

    let Some(event) = event else {
        return Ok(());
    };
    // dbg!(&event);
    let line = match event? {
        Event::Recognized(_, result, _, _, _) => {
            Some(Line::Recognised(result.text))
        }
        Event::Recognizing(_, result, _, _, _) => {
            Some(Line::Recognising(result.text))
        }
        event => {
            info!("Unhandled event: {event:?}");
            None
        }
    };

    if let Some(line) = line
        && tx.try_send(line).is_err()
    {
        warn!("Line channel full");
    }
    Ok(())
}
