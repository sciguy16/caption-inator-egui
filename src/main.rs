#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(rustdoc::missing_crate_level_docs)] // it's an example

use clap::Parser;
use color_eyre::{eyre::eyre, Result};
use eframe::epaint::text::{FontInsert, InsertFontFamily};
use egui::{
    scroll_area::{ScrollBarVisibility, ScrollSource},
    Align, Color32, FontFamily, FontId, Layout, Modal, Pos2, Rect, RichText,
    TextStyle, Vec2, ViewportBuilder, ViewportCommand, ViewportId,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    ops::DerefMut,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tokio::sync::{mpsc, oneshot};

#[macro_use]
extern crate tracing;

mod config;
mod input;
mod listener;

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

#[derive(Parser)]
struct Args {
    #[clap(long, help = "Path to config file")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    init_tracing();

    let args = Args::parse();
    let config = Config::load(&args.config)?;

    let (tx, rx) = mpsc::channel(10);
    let (control_tx, control_rx) = mpsc::channel(5);

    info!("Starting captioninator");
    let auth = match (config.region.clone(), config.key.clone()) {
        (Some(region), Some(key)) => listener::Auth { region, key },
        _ => Err(eyre!("Region and key are required for Azure listener"))?,
    };
    listener::start(tx.clone(), control_rx, auth, config.clone());

    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default().with_fullscreen(true),
        ..Default::default()
    };

    let app = MyApp::new(rx, control_tx).await?;

    eframe::run_native(
        "egui example: custom font",
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

struct MyApp {
    text_buffer: VecDeque<String>,
    active_line: Option<String>,
    rx: mpsc::Receiver<Line>,
    control_state: Arc<Mutex<ControlState>>,
}

struct ControlState {
    state: State,
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
            RunState::Stopped => RunState::Running,
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
            RunState::Stopped => RunState::Test,
        };
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

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
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

impl MyApp {
    async fn new(
        rx: mpsc::Receiver<Line>,
        control_tx: mpsc::Sender<ControlMessage>,
    ) -> Result<Self> {
        let wordlist = {
            let (tx, rx) = oneshot::channel();
            control_tx
                .send(ControlMessage::GetWordlist(tx))
                .await
                .unwrap();
            rx.await?
        };

        Ok(Self {
            text_buffer: VecDeque::with_capacity(LINE_BUFFER_SIZE * 2),
            active_line: None,
            rx,
            control_state: Arc::new(Mutex::new(ControlState {
                state: State::default(),
                fullscreen_font_size: 100.0,
                subtitle_font_size: 50.0,
                subtitle_height_proportion: 0.2,
                dark_mode_enabled: true,
                dark_mode_requested: true,
                display_mode: DisplayMode::default(),
                control_tx,
                run_state: RunState::default(),
                wordlist_options: wordlist.options,
                wordlist: wordlist.current,
                request_close: AtomicBool::default(),
            })),
        })
    }
}

#[derive(PartialEq, Eq, Default)]
enum State {
    #[default]
    Normal,
    Config,
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.show_viewport_deferred(
            ViewportId::from_hash_of("controls-window"),
            ViewportBuilder::default().with_title("Caption controls"),
            {
                let control_state = Arc::clone(&self.control_state);
                move |ctx, _| config::window(ctx, Arc::clone(&control_state))
            },
        );

        while let Ok(line) = self.rx.try_recv() {
            match line {
                Line::Recognising(line) => {
                    self.active_line = Some(line);
                }
                Line::Recognised(line) => {
                    self.text_buffer.push_back(line);
                    self.active_line.take();
                }
            }
        }

        let mut control_state = self.control_state.lock().unwrap();

        if control_state.request_close.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }

        let limit = match control_state.display_mode {
            DisplayMode::Fullscreen => LINE_BUFFER_SIZE,
            DisplayMode::Subtitle => 4,
        };
        while self.text_buffer.len() > limit {
            self.text_buffer.pop_front();
        }

        let max_height = match control_state.display_mode {
            DisplayMode::Fullscreen => f32::INFINITY,
            DisplayMode::Subtitle => {
                ctx.screen_rect().height()
                    * control_state.subtitle_height_proportion
            }
        };
        let top = match control_state.display_mode {
            DisplayMode::Fullscreen => 0.0,
            DisplayMode::Subtitle => ctx.screen_rect().height() - max_height,
        };

        input::process(ctx, control_state.deref_mut());

        let base_theme = if control_state.dark_mode_requested {
            catppuccin_egui::MOCHA
        } else {
            catppuccin_egui::LATTE
        };
        if control_state.dark_mode_enabled != control_state.dark_mode_requested
        {
            catppuccin_egui::set_theme(ctx, base_theme);
            control_state.dark_mode_enabled = control_state.dark_mode_requested;
        }

        let bg_fill = if control_state.display_mode == DisplayMode::Subtitle {
            Color32::GREEN
        } else {
            base_theme.base
        };

        // Override the panel fill for just the subtitles panel
        ctx.style_mut(|styles| {
            styles.visuals.panel_fill = bg_fill;
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.advance_cursor_after_rect(Rect::from_min_size(
                Pos2::ZERO,
                top * Vec2::DOWN,
            ));
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .scroll_source(ScrollSource::NONE)
                .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        for line in
                            self.text_buffer.iter().chain(&self.active_line)
                        {
                            ui.label(
                                RichText::new(line)
                                    .size(control_state.font_size()),
                            );
                        }
                    });
                });
        });
        // and then set it back afterwards
        ctx.style_mut(|styles| {
            styles.visuals.panel_fill = base_theme.base;
        });

        if control_state.state == State::Config {
            Modal::new("config-modal".into())
                .show(ctx, |ui| config::show(ui, control_state.deref_mut()));
        }

        ctx.request_repaint();
    }
}

#[derive(Clone, Deserialize)]
pub struct Config {
    pub region: Option<String>,
    pub key: Option<String>,
    pub wordlist_dir: Option<PathBuf>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        toml::de::from_str(&content).map_err(Into::into)
    }
}
