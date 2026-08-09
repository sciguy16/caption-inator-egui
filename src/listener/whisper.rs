use crate::{
    ControlMessage, Line, Result, RunState, config::Config,
    listener::SetupState,
};
use color_eyre::eyre::eyre;
use futures::SinkExt;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_tungstenite::tungstenite::protocol::Message;
use uuid::Uuid;

pub async fn do_run(
    tx: &mpsc::Sender<Line>,
    control_rx: &mut mpsc::Receiver<ControlMessage>,
    setup_state: &mut SetupState,
    config: &Config,
) -> Result<RunState> {
    let mut runner = WhisperRunner {
        tx,
        control_rx,
        setup_state,
        config,
        uuid: Uuid::new_v4(),
        server_status: ServerStatus::default(),
        last_start: -1.0,
    };
    runner.run().await
}

struct WhisperRunner<'this> {
    tx: &'this mpsc::Sender<Line>,
    control_rx: &'this mut mpsc::Receiver<ControlMessage>,
    setup_state: &'this mut SetupState,
    config: &'this Config,
    uuid: Uuid,
    server_status: ServerStatus,
    last_start: f32,
}

#[derive(Copy, Clone, Default, PartialEq, Eq)]
enum ServerStatus {
    #[default]
    Waiting,
    Ready,
    Disconnected,
}

impl<'this> WhisperRunner<'this> {
    pub async fn run(&mut self) -> Result<RunState> {
        let (Some(host), Some(port)) =
            (self.config.whisper_host.as_ref(), self.config.whisper_port)
        else {
            return Err(eyre!("Missing whisper host & port configuration"));
        };
        let opts = ClientOptions {
            uid: self.uuid,
            ..ClientOptions::default()
        };

        // let mut azure_config = azure_speech::recognizer::Config::default()
        //     .set_language(langauge_from_language(&setup_state.language))
        //     .set_profanity(azure_speech::recognizer::Profanity::Raw);

        // if let (Some(wordlist_dir), Some(wordlist_file)) =
        //     (&config.wordlist_dir, &setup_state.wordlist)
        // {
        //     let wordlist_path = wordlist_dir.join(wordlist_file.as_ref());
        //     let wordlist = std::fs::read_to_string(wordlist_path)?;
        //     let wordlist = wordlist
        //         .lines()
        //         .filter(|line| !line.is_empty())
        //         .map(String::from)
        //         .collect();
        //     azure_config = azure_config.set_phrases(wordlist);
        // }

        'reconnection: loop {
            let (mut client, _response) =
                tokio_tungstenite::connect_async(format!("ws://{host}:{port}"))
                    .await?;

            client.send(serde_json::to_string(&opts)?.into()).await?;

            let mut audio_stream =
                // super::listen_from_default_input("f32le").await?;
                super::listen_from_default_input("s16le").await?;

            info!("... Starting to listen from microphone ...");

            let new_state = loop {
                tokio::select! {
                    msg = client.next() => {
                        let Some(msg) = msg else {break 'reconnection};
                        let msg = msg?;
                        self.handle_msg(msg);
                    }
                    msg = self.control_rx.recv() => {
                        let Some(msg) = msg else {break 'reconnection};
                        match msg {
                            ControlMessage::SetState(RunState::Running) => {}
                            ControlMessage::SetState(new_state) => {
                                break new_state;
                            }
                            other => {
                                super::handle_lang_and_wordlist(
                                    other, self.setup_state, self.config,
                                );
                            }
                        }
                    }
                    chunk = audio_stream.next() => {
                        let Some(chunk) = chunk else {break 'reconnection};

                        if self.server_status == ServerStatus::Ready {
                            client.send(chunk.into()).await?;
                        }
                    }
                }
                if self.server_status == ServerStatus::Disconnected {
                    break RunState::Stopped;
                }
            };

            if let Err(err) = client.close(None).await {
                warn!("Disconnection failed: {err}");
            }

            if new_state != RunState::Running {
                info!("whisper speech client shut down");
                return Ok(new_state);
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
            info!("Reconnecting");
        }

        Ok(RunState::Stopped)
    }

    fn handle_msg(&mut self, msg: Message) {
        let msg = match msg {
            Message::Text(msg) => msg,
            Message::Ping(_) => {
                return;
            }
            Message::Close(code) => {
                warn!("Close frame received: {code:?}");
                self.server_status = ServerStatus::Disconnected;
                return;
            }
            other => {
                warn!("Non-text message received: {other:?}");
                return;
            }
        };
        let msg = match serde_json::from_str::<ReceivedMessage>(&msg) {
            Ok(msg) => msg,
            Err(err) => {
                warn!("Deserialisation failed: {err}\nMessage: {msg}");
                return;
            }
        };

        if msg.uid != self.uuid {
            warn!("Invalid message UUID");
            return;
        }

        match &msg.message {
            Some(msg) if msg == "SERVER_READY" => {
                self.server_status = ServerStatus::Ready;
                info!("Server ready!");
            }
            Some(msg) if msg == "DISCONNECT" => {
                warn!("Server disconnected!");
                self.server_status = ServerStatus::Disconnected;
            }
            Some(other) => {
                warn!("Unhandled message: {other}");
            }
            None => {}
        }

        if let Some(status) = &msg.status {
            info!("{status}: {msg:?}");
        }

        if let Some(language) = msg.language {
            info!(?language);
        }
        if let Some(language_prob) = msg.language_prob {
            info!(?language_prob);
        }
        if let Some(backend) = msg.backend {
            info!(?backend);
        }
        if let Some(translated_segments) = msg.translated_segments {
            info!(?translated_segments);
        }

        for segment in msg.segments {
            if segment.text == " ..." {
                print!(".");
                continue;
            }
            trace!(?segment.completed,?segment.text);
            let start = segment.start.parse::<f32>().unwrap_or_default();
            let line = if segment.completed {
                if start > self.last_start {
                    self.last_start = start;
                    Some(Line::Recognised(segment.text))
                } else {
                    None
                }
            } else {
                Some(Line::Recognising(segment.text))
            };
            if let Some(line) = line
                && self.tx.try_send(line).is_err()
            {
                warn!("Line channel full");
            }
        }
    }
}

// fn langauge_from_language(lang: &str) -> azure_speech::recognizer::Language {
//     match lang {
//         "en-GB" => azure_speech::recognizer::Language::EnGb,
//         "en-IE" => azure_speech::recognizer::Language::EnIe,
//         "en-US" => azure_speech::recognizer::Language::EnUs,
//         "ja-JP" => azure_speech::recognizer::Language::JaJp,
//         _ => azure_speech::recognizer::Language::EnGb,
//     }
// }

// async fn handle_event(
//     event: Option<Result<azure_speech::recognizer::Event, azure_speech::Error>>,
//     tx: &mpsc::Sender<Line>,
// ) -> Result<(), azure_speech::Error> {
//     use azure_speech::recognizer::Event;

//     let Some(event) = event else {
//         return Ok(());
//     };
//     // dbg!(&event);
//     let line = match event? {
//         Event::Recognized(_, result, _, _, _) => {
//             Some(Line::Recognised(result.text))
//         }
//         Event::Recognizing(_, result, _, _, _) => {
//             Some(Line::Recognising(result.text))
//         }
//         event => {
//             info!("Unhandled event: {event:?}");
//             None
//         }
//     };

//     if let Some(line) = line
//         && tx.try_send(line).is_err()
//     {
//         warn!("Line channel full");
//     }
//     Ok(())
// }

#[derive(Debug, Deserialize, Serialize)]
struct ClientOptions {
    uid: Uuid,
    /// The selected language for transcription.
    language: Option<String>,
    /// Whether to translate or transcribe
    task: String,
    /// The whisper model to use (e.g., "small", "medium", "large").
    /// Default is "small".
    model: String,
    /// Whether to enable voice activity detection.
    use_vad: bool,
    /// Segments with no speech probability above this threshold will be
    /// discarded. Defaults to 0.45.
    no_speech_thresh: f64,
    // / Whether to clip audio with no valid segments. Defaults to False.
    // clip_audio: bool,
    /// Number of repeated outputs before considering it as a valid segment.
    /// Defaults to 10.
    same_output_threshold: i64,
    // hotwords: self.hotwords,
    // enable_diarization: self.enable_diarization,
    // max_speakers: self.max_speakers,
    // / Optional text to provide context to the model (e.g. domain
    // / vocabulary or names).
    // initial_prompt: Option<String>,
    ///Optional voice-activity-detection parameters passed to the server
    /// backend.
    vad_parameters: Option<HashMap<String, String>>,
    /// Audio format. Must be one of "float32", "int16", "uint8"
    audio_format: String,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            uid: Uuid::default(),
            language: Some("en".into()),
            task: "transcribe".into(),
            model: "small.en".into(),
            use_vad: false,
            no_speech_thresh: 0.6,
            // clip_audio: true,
            same_output_threshold: 10,
            // initial_prompt: None,
            vad_parameters: None,
            audio_format: "int16".into(),
        }
    }
}

// No idea what the schema actually looks like, so this is a terrible
// implementation that looks like the python fiasco
#[derive(Debug, Deserialize)]
struct ReceivedMessage {
    uid: Uuid,
    status: Option<String>,
    message: Option<String>,
    backend: Option<String>,
    language: Option<String>,
    language_prob: Option<f64>,
    #[serde(default)]
    segments: Vec<Segment>,
    translated_segments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Segment {
    start: String,
    #[expect(unused)]
    end: String,
    text: String,
    completed: bool,
}
