use crate::{
    State, MAX_FONT, MAX_SUBTITLE_HEIGHT, MIN_FONT, MIN_SUBTITLE_HEIGHT,
};
use egui::{Context, Key};

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

    *app.font_size_mut() = app.font_size().clamp(MIN_FONT, MAX_FONT);
    app.subtitle_height_proportion = app
        .subtitle_height_proportion
        .clamp(MIN_SUBTITLE_HEIGHT, MAX_SUBTITLE_HEIGHT);
}
