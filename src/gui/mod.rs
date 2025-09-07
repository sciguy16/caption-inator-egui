use crate::xrandr::MonitorPositions;
use crate::{
    ControlMessage, ControlState, DisplayMode, Line, RunState, LINE_BUFFER_SIZE,
};
use color_eyre::Result;
use egui::{
    scroll_area::{ScrollBarVisibility, ScrollSource},
    Align, Color32, Layout, Modal, Pos2, Rect, RichText, Vec2, ViewportBuilder,
    ViewportCommand, ViewportId,
};
use std::{
    collections::VecDeque,
    ops::DerefMut,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tokio::sync::{mpsc, oneshot};

mod controls;
mod input;

pub struct MyApp {
    text_buffer: VecDeque<String>,
    active_line: Option<String>,
    rx: mpsc::Receiver<Line>,
    monitor_positions: MonitorPositions,
    control_state: Arc<Mutex<ControlState>>,
}

impl MyApp {
    pub async fn new(
        rx: mpsc::Receiver<Line>,
        control_tx: mpsc::Sender<ControlMessage>,
        monitor_positions: crate::xrandr::MonitorPositions,
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
            monitor_positions,
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
pub enum State {
    #[default]
    Normal,
    Config,
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.show_viewport_deferred(
            ViewportId::from_hash_of("controls-window"),
            ViewportBuilder::default()
                .with_title("Caption controls")
                .with_position(self.monitor_positions.internal),
            {
                let control_state = Arc::clone(&self.control_state);
                move |ctx, _| controls::window(ctx, Arc::clone(&control_state))
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
                .show(ctx, |ui| controls::show(ui, control_state.deref_mut()));
        }

        ctx.request_repaint();
    }
}
