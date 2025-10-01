use crate::{
    MAX_FONT, MAX_SUBTITLE_HEIGHT, MIN_FONT, MIN_SUBTITLE_HEIGHT, gui::State,
};
use egui::{Context, Key, ViewportCommand};

pub fn process(ctx: &Context, app: &mut crate::ControlState) {
    if ctx.input(|i| i.key_pressed(Key::F1)) {
        app.state = State::Config;
    }
    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        app.state = State::Normal;
    }

    if ctx.input(|i| i.key_pressed(Key::Minus)) {
        *app.font_size_mut() -= 1.0;
    }
    if ctx.input(|i| i.key_pressed(Key::Equals)) {
        *app.font_size_mut() += 1.0;
    }

    if ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
        app.subtitle_height_proportion += 0.1;
    }
    if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
        app.subtitle_height_proportion -= 0.1;
    }

    if ctx.input(|i| i.key_pressed(Key::D)) {
        app.dark_mode_requested = !app.dark_mode_enabled;
    }

    if ctx.input(|i| i.key_pressed(Key::M)) {
        app.display_mode.swap();
    }

    if ctx.input(|i| i.key_pressed(Key::Space)) {
        app.toggle_running();
    }

    if ctx.input(|i| i.key_pressed(Key::T)) {
        app.toggle_test_mode();
    }

    if ctx.input(|i| i.key_pressed(Key::H)) {
        app.toggle_holding_slide();
    }

    if ctx.input(|i| i.key_pressed(Key::F11)) {
        toggle_fullscreen(ctx);
    }

    *app.font_size_mut() = app.font_size().clamp(MIN_FONT, MAX_FONT);
    app.subtitle_height_proportion = app
        .subtitle_height_proportion
        .clamp(MIN_SUBTITLE_HEIGHT, MAX_SUBTITLE_HEIGHT);

    if ctx.input(|input| input.viewport().close_requested()) {
        app.stop();
    }
}

fn toggle_fullscreen(ctx: &egui::Context) {
    let is_fullscreen = ctx.input(|input_state| {
        input_state.viewport().fullscreen.unwrap_or(false)
    });
    ctx.send_viewport_cmd(ViewportCommand::Fullscreen(!is_fullscreen));
}
