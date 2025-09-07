#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(rustdoc::missing_crate_level_docs)] // it's an example

use clap::Parser;
use color_eyre::{eyre::eyre, Result};
use eframe::epaint::text::{FontInsert, InsertFontFamily};
use egui::{FontFamily, FontId, TextStyle, ViewportBuilder};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    str::FromStr,
    sync::{atomic::AtomicBool, Arc},
};
use tokio::sync::{mpsc, oneshot};

#[macro_use]
extern crate tracing;

mod config;
mod gui;
mod listener;
mod xrandr;

const NOTO_SANS: &[u8] = include_bytes!("../fonts/NotoSans-Regular.ttf");

const LINE_BUFFER_SIZE: usize = 30;

const MIN_FONT: f32 = 30.0;
const MAX_FONT: f32 = 400.0;
const MIN_SUBTITLE_HEIGHT: f32 = 0.1;
const MAX_SUBTITLE_HEIGHT: f32 = 0.9;

const PREFIX_RECOGNISING: &str = "RECOGNIZING: ";
const PREFIX_RECOGNISED: &str = "RECOGNIZED: ";
// https://learn.microsoft.com/en-us/azure/ai-services/speech-service/language-support?tabs=stt
const LANGUAGE_OPTIONS: &[&str] = &["en-GB", "en-IE", "en-US", "ja-JP"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
enum Line {
    Recognising(String),
    Recognised(String),
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize)]
enum RunState {
    #[default]
    Stopped,
    Running,
    Test,
    HoldingSlide,
}

#[derive(Debug)]
enum ControlMessage {
    SetState(RunState),
    SetWordlist(Option<Arc<str>>),
    GetWordlist(oneshot::Sender<Wordlist>),
}

impl FromStr for Line {
    type Err = color_eyre::Report;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if let Some(line) = s.strip_prefix(PREFIX_RECOGNISING) {
            Ok(Self::Recognising(line.into()))
        } else if let Some(line) = s.strip_prefix(PREFIX_RECOGNISED) {
            Ok(Self::Recognised(line.into()))
        } else {
            Err(eyre!("Invalid input"))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Wordlist {
    options: Vec<Arc<str>>,
    current: Option<Arc<str>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    init_tracing();

    let args = config::Args::parse();
    let config = config::Config::load(args.config)?;

    let (tx, rx) = mpsc::channel(10);
    let (control_tx, control_rx) = mpsc::channel(5);

    info!("Starting captioninator");
    let auth = match (config.region.clone(), config.key.clone()) {
        (Some(region), Some(key)) => listener::Auth { region, key },
        _ => Err(eyre!("Region and key are required for Azure listener"))?,
    };
    listener::start(tx.clone(), control_rx, auth, config.clone());

    // Use set position to position the window on the secondary display.
    // The position is derived from a call to xrandr
    let monitor_positions = xrandr::monitor_positions();
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_fullscreen(true)
            .with_position(monitor_positions.external),
        ..Default::default()
    };

    let mut app =
        gui::MyApp::new(rx, config, control_tx, monitor_positions).await?;

    eframe::run_native(
        "captioninator",
        options,
        Box::new(|cc| {
            cc.egui_ctx.all_styles_mut(|style| {
                let text_styles = {
                    use FontFamily::Proportional;
                    use TextStyle::*;
                    [
                        (Heading, FontId::new(80.0, Proportional)),
                        (
                            Name("Heading2".into()),
                            FontId::new(25.0, Proportional),
                        ),
                        (
                            Name("Context".into()),
                            FontId::new(23.0, Proportional),
                        ),
                        (Body, FontId::new(50.0, Proportional)),
                        (Monospace, FontId::new(14.0, Proportional)),
                        (Button, FontId::new(40.0, Proportional)),
                        (Small, FontId::new(30.0, Proportional)),
                    ]
                }
                .into();

                style.text_styles = text_styles;
            });
            catppuccin_egui::set_theme(&cc.egui_ctx, catppuccin_egui::MOCHA);

            replace_fonts(&cc.egui_ctx);
            add_font(&cc.egui_ctx);
            egui_extras::install_image_loaders(&cc.egui_ctx);

            if let Some(storage) = cc.storage {
                app.load_control_state(storage);
            }

            Ok(Box::new(app))
        }),
    )
    .unwrap();
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::{filter::LevelFilter, EnvFilter};

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::DEBUG.into())
                .from_env_lossy(),
        )
        .with_line_number(true)
        .init();
}

// Demonstrates how to add a font to the existing ones
fn add_font(ctx: &egui::Context) {
    ctx.add_font(FontInsert::new(
        "my_font",
        egui::FontData::from_static(NOTO_SANS),
        vec![
            InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority: egui::epaint::text::FontPriority::Highest,
            },
            InsertFontFamily {
                family: egui::FontFamily::Monospace,
                priority: egui::epaint::text::FontPriority::Lowest,
            },
        ],
    ));
}

// Demonstrates how to replace all fonts.
fn replace_fonts(ctx: &egui::Context) {
    // Start with the default fonts (we will be adding to them rather than replacing them).
    let mut fonts = egui::FontDefinitions::default();

    // Install my own font (maybe supporting non-latin characters).
    // .ttf and .otf files supported.
    fonts.font_data.insert(
        "my_font".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(NOTO_SANS)),
    );

    // Put my font first (highest priority) for proportional text:
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "my_font".to_owned());

    // Put my font as last fallback for monospace:
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push("my_font".to_owned());

    // Tell egui to use these fonts:
    ctx.set_fonts(fonts);
}

struct ControlState {
    state: gui::State,
    fullscreen_font_size: f32,
    subtitle_font_size: f32,
    subtitle_height_proportion: f32,
    dark_mode_enabled: bool,
    dark_mode_requested: bool,
    display_mode: DisplayMode,
    control_tx: mpsc::Sender<ControlMessage>,
    run_state: RunState,
    wordlist_options: Vec<Arc<str>>,
    wordlist: Option<Arc<str>>,
    request_close: AtomicBool,
    image_options: Vec<Arc<str>>,
    selected_image: Option<Arc<str>>,
}

impl ControlState {
    const fn font_size(&self) -> f32 {
        match self.display_mode {
            DisplayMode::Fullscreen => self.fullscreen_font_size,
            DisplayMode::Subtitle => self.subtitle_font_size,
        }
    }

    const fn font_size_mut(&mut self) -> &mut f32 {
        match self.display_mode {
            DisplayMode::Fullscreen => &mut self.fullscreen_font_size,
            DisplayMode::Subtitle => &mut self.subtitle_font_size,
        }
    }

    fn toggle_running(&mut self) {
        self.run_state = match self.run_state {
            RunState::Running | RunState::Test => RunState::Stopped,
            RunState::Stopped | RunState::HoldingSlide => RunState::Running,
        };
        if let Err(err) = self
            .control_tx
            .try_send(ControlMessage::SetState(self.run_state))
        {
            error!("{err}");
        }
    }

    fn toggle_test_mode(&mut self) {
        self.run_state = match self.run_state {
            RunState::Running | RunState::Test => RunState::Stopped,
            RunState::Stopped | RunState::HoldingSlide => RunState::Test,
        };
        if let Err(err) = self
            .control_tx
            .try_send(ControlMessage::SetState(self.run_state))
        {
            error!("{err}");
        }
    }

    fn toggle_holding_slide(&mut self) {
        self.run_state = match self.run_state {
            RunState::HoldingSlide => RunState::Stopped,
            RunState::Running | RunState::Stopped | RunState::Test => {
                RunState::HoldingSlide
            }
        };
        dbg!(self.run_state);
        dbg!(&self.selected_image);
        if let Err(err) = self
            .control_tx
            .try_send(ControlMessage::SetState(self.run_state))
        {
            error!("{err}");
        }
    }

    fn update_wordlist(&mut self) {
        if let Err(err) = self
            .control_tx
            .try_send(ControlMessage::SetWordlist(self.wordlist.clone()))
        {
            error!("{err}");
        }
    }
}

#[derive(
    Copy, Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize,
)]
enum DisplayMode {
    #[default]
    Fullscreen,
    Subtitle,
}

impl DisplayMode {
    fn swap(&mut self) {
        *self = match self {
            Self::Fullscreen => Self::Subtitle,
            Self::Subtitle => Self::Fullscreen,
        }
    }
}

fn list_directory(dir: &Path) -> Vec<Arc<str>> {
    let mut options = Vec::new();

    for entry in dir
        .read_dir()
        .unwrap_or_else(|err| panic!("Path: {}\n{:?}", dir.display(), err))
    {
        let Ok(entry) = entry else { continue };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_file() {
            let Ok(file_name) = entry.file_name().into_string() else {
                continue;
            };

            if !file_name.starts_with('.') {
                options.push(file_name.into());
            }
        }
    }

    options
}
