use crate::DisplayMode;
use egui::{
    scroll_area::{ScrollBarVisibility, ScrollSource},
    Align, Color32, Frame, Layout, Margin, Rect, RichText, Vec2,
};

//TODO figure out a better way to shift the subtitle mode away from the
// PiP camera view
const PADDING_LEFT: i8 = 50;
const PADDING_RIGHT: i8 = 127;
const PADDING_VERTICAL: i8 = 10;

pub fn show(app: &mut crate::gui::MyApp, ctx: &egui::Context) {
    let mut control_state = app.control_state.lock().unwrap();

    let max_height = match control_state.display_mode {
        DisplayMode::Fullscreen => f32::INFINITY,
        DisplayMode::Subtitle => {
            ctx.content_rect().height()
                * control_state.subtitle_height_proportion
        }
    };

    let top = match control_state.display_mode {
        DisplayMode::Fullscreen => 0.0,
        DisplayMode::Subtitle => ctx.content_rect().height() - max_height,
    };

    let base_theme = if control_state.dark_mode_requested {
        catppuccin_egui::MOCHA
    } else {
        catppuccin_egui::LATTE
    };
    if control_state.dark_mode_enabled != control_state.dark_mode_requested {
        catppuccin_egui::set_theme(ctx, base_theme);
        control_state.dark_mode_enabled = control_state.dark_mode_requested;
    }

    let (max_width, bg_fill) =
        if control_state.display_mode == DisplayMode::Subtitle {
            (1350.0, Color32::GREEN)
        } else {
            (ctx.content_rect().width(), base_theme.base)
        };

    // Override the panel fill for just the subtitles panel
    ctx.style_mut(|styles| {
        styles.visuals.panel_fill = bg_fill;
    });
    egui::CentralPanel::default()
        .frame(
            Frame::default()
                .inner_margin(Margin {
                    left: PADDING_LEFT,
                    right: PADDING_RIGHT,
                    top: PADDING_VERTICAL,
                    bottom: PADDING_VERTICAL,
                })
                .fill(bg_fill),
        )
        .show(ctx, |ui| {
            ui.advance_cursor_after_rect(Rect::from_min_size(
                ui.next_widget_position(),
                top * Vec2::DOWN,
            ));
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .scroll_source(ScrollSource::NONE)
                .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
                .auto_shrink(false)
                .max_width(max_width)
                .show(ui, |ui| {
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        for line in
                            app.text_buffer.iter().chain(&app.active_line)
                        {
                            ui.label(
                                RichText::new(line)
                                    .size(control_state.font_size()),
                            );
                        }
                    });
                    // });
                });
        });
    // and then set it back afterwards
    ctx.style_mut(|styles| {
        styles.visuals.panel_fill = base_theme.base;
    });
}
