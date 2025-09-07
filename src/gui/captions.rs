use crate::DisplayMode;
use egui::{
    scroll_area::{ScrollBarVisibility, ScrollSource},
    Align, Color32, Layout, Pos2, Rect, RichText, Vec2,
};

pub fn show(app: &mut crate::gui::MyApp, ctx: &egui::Context) {
    let mut control_state = app.control_state.lock().unwrap();

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

    let base_theme = if control_state.dark_mode_requested {
        catppuccin_egui::MOCHA
    } else {
        catppuccin_egui::LATTE
    };
    if control_state.dark_mode_enabled != control_state.dark_mode_requested {
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
                    for line in app.text_buffer.iter().chain(&app.active_line) {
                        ui.label(
                            RichText::new(line).size(control_state.font_size()),
                        );
                    }
                });
            });
    });
    // and then set it back afterwards
    ctx.style_mut(|styles| {
        styles.visuals.panel_fill = base_theme.base;
    });
}
