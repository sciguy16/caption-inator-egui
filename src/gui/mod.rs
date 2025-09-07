use crate::{
    xrandr::MonitorPositions, ControlMessage, ControlState, DisplayMode, Line,
    RunState, LINE_BUFFER_SIZE,
};
use color_eyre::Result;
use egui::{Modal, ViewportBuilder, ViewportCommand, ViewportId};
use std::{
    collections::VecDeque,
    ops::DerefMut,
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
    sync::Mutex,
};
use tokio::sync::{mpsc, oneshot};

mod captions;
mod controls;
mod holding_image;
mod input;

pub struct MyApp {
    text_buffer: VecDeque<String>,
    active_line: Option<String>,
    rx: mpsc::Receiver<Line>,
    monitor_positions: MonitorPositions,
    config: crate::config::Config,
    control_state: Arc<Mutex<ControlState>>,
}

impl MyApp {
    pub async fn new(
        rx: mpsc::Receiver<Line>,
        config: crate::config::Config,
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

        let image_options = config
            .images_dir
            .as_deref()
            .map(crate::list_directory)
            .unwrap_or_default();
        let selected_image = image_options.first().cloned();

        Ok(Self {
            text_buffer: VecDeque::with_capacity(LINE_BUFFER_SIZE * 2),
            active_line: None,
            rx,
            monitor_positions,
            config,
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
                image_options,
                selected_image,
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

        input::process(ctx, control_state.deref_mut());

        let limit = match control_state.display_mode {
            DisplayMode::Fullscreen => LINE_BUFFER_SIZE,
            DisplayMode::Subtitle => 4,
        };
        while self.text_buffer.len() > limit {
            self.text_buffer.pop_front();
        }

        if control_state.state == State::Config {
            Modal::new("config-modal".into())
                .show(ctx, |ui| controls::show(ui, control_state.deref_mut()));
        }

        let run_state = control_state.run_state;
        let selected_image = control_state.selected_image.clone();
        drop(control_state);

        if run_state == RunState::HoldingSlide
            && let Some(selected_image) = selected_image
            && let Some(images_dir) = self.config.images_dir.as_deref()
        {
            holding_image::show(ctx, images_dir, &selected_image)
        } else {
            captions::show(self, ctx);
        }

        ctx.request_repaint();
    }
}
