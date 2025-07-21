#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(rustdoc::missing_crate_level_docs)] // it's an example

use eframe::epaint::text::{FontInsert, InsertFontFamily};
use egui::{
    scroll_area::{ScrollBarVisibility, ScrollSource},
    Align, Color32, FontFamily, FontId, Layout, Modal, Pos2, Rect, RichText,
    TextStyle, Vec2,
};
use std::{collections::VecDeque, sync::mpsc, time::Duration};

mod config;
mod input;

const NOTO_SANS: &[u8] = include_bytes!("../fonts/NotoSans-Regular.ttf");

const LINE_BUFFER_SIZE: usize = 30;

const MIN_FONT: f32 = 30.0;
const MAX_FONT: f32 = 400.0;
const MIN_SUBTITLE_HEIGHT: f32 = 0.1;
const MAX_SUBTITLE_HEIGHT: f32 = 0.9;

fn main() -> eframe::Result {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || background_sender(tx));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_fullscreen(true),
        ..Default::default()
    };

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
            Ok(Box::new(MyApp::new(cc, rx)))
        }),
    )
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
    rx: mpsc::Receiver<String>,
    state: State,
    fullscreen_font_size: f32,
    subtitle_font_size: f32,
    subtitle_height_proportion: f32,
    dark_mode_enabled: bool,
    dark_mode_requested: bool,
    display_mode: DisplayMode,
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
    fn new(
        cc: &eframe::CreationContext<'_>,
        rx: mpsc::Receiver<String>,
    ) -> Self {
        replace_fonts(&cc.egui_ctx);
        add_font(&cc.egui_ctx);

        Self {
            text_buffer: VecDeque::with_capacity(LINE_BUFFER_SIZE * 2),
            rx,
            state: State::default(),
            fullscreen_font_size: 100.0,
            subtitle_font_size: 50.0,
            subtitle_height_proportion: 0.2,
            dark_mode_enabled: true,
            dark_mode_requested: true,
            display_mode: DisplayMode::default(),
        }
    }

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
}

#[derive(PartialEq, Eq, Default)]
enum State {
    #[default]
    Normal,
    Config,
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(line) = self.rx.try_recv() {
            self.text_buffer.push_back(line);
        }

        let limit = match self.display_mode {
            DisplayMode::Fullscreen => LINE_BUFFER_SIZE,
            DisplayMode::Subtitle => 2,
        };
        while self.text_buffer.len() > limit {
            self.text_buffer.pop_front();
        }

        let max_height = match self.display_mode {
            DisplayMode::Fullscreen => f32::INFINITY,
            DisplayMode::Subtitle => {
                ctx.screen_rect().height() * self.subtitle_height_proportion
            }
        };
        let top = match self.display_mode {
            DisplayMode::Fullscreen => 0.0,
            DisplayMode::Subtitle => ctx.screen_rect().height() - max_height,
        };

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
                        for line in &self.text_buffer {
                            ui.label(
                                RichText::new(line).size(self.font_size()),
                            );
                        }
                    });
                });
        });

        if self.state == State::Config {
            Modal::new("config-modal".into())
                .show(ctx, |ui| config::show(ui, self));
        }

        input::process(ctx, self);

        let base_theme = if self.dark_mode_requested {
            catppuccin_egui::MOCHA
        } else {
            catppuccin_egui::LATTE
        };
        if self.dark_mode_enabled != self.dark_mode_requested {
            catppuccin_egui::set_theme(ctx, base_theme);
            self.dark_mode_enabled = self.dark_mode_requested;
        }
        let bg_fill = if self.display_mode == DisplayMode::Subtitle {
            Color32::GREEN
        } else {
            base_theme.base
        };
        ctx.all_styles_mut(|styles| {
            styles.visuals.window_fill = bg_fill;
            styles.visuals.panel_fill = bg_fill;
        });

        ctx.request_repaint();
    }
}

fn background_sender(tx: mpsc::Sender<String>) {
    for repeat in 1.. {
        tx.send("game ".repeat(repeat)).unwrap();
        std::thread::sleep(Duration::from_millis(500));
    }
}
