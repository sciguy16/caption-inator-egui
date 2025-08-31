use crate::{
    DisplayMode, MAX_FONT, MAX_SUBTITLE_HEIGHT, MIN_FONT, MIN_SUBTITLE_HEIGHT,
    RunState,
};
use egui::{Button, ComboBox, RichText, Slider, Ui};
use std::{
    ops::DerefMut,
    sync::{Arc, Mutex, atomic::Ordering},
};

pub fn show(ui: &mut Ui, app: &mut crate::ControlState) {
    ui.heading(RichText::new("game").size(50.0));

    ui.add(
        Slider::new(app.font_size_mut(), MIN_FONT..=MAX_FONT).text("Font size"),
    );
    ui.add(
        Slider::new(
            &mut app.subtitle_height_proportion,
            MIN_SUBTITLE_HEIGHT..=MAX_SUBTITLE_HEIGHT,
        )
        .text("Subtitle height"),
    );

    ui.checkbox(&mut app.dark_mode_requested, "Dark Mode [d]");

    ui.horizontal(|ui| {
        ui.label("Display mode [m]");
        ComboBox::from_id_salt("display_mode")
            .selected_text(format!("{:?}", app.display_mode))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut app.display_mode,
                    DisplayMode::Fullscreen,
                    "Full screen",
                );
                ui.selectable_value(
                    &mut app.display_mode,
                    DisplayMode::Subtitle,
                    "Subtitle",
                );
            });
    });

    ui.horizontal(|ui| {
        let current_wordlist = app.wordlist.as_deref().unwrap_or("None");
        let before = app.wordlist.clone();
        ui.label("Wordlist");
        ComboBox::from_id_salt("wordlist")
            .selected_text(current_wordlist)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.wordlist, None, "None");
                for option in &app.wordlist_options {
                    ui.selectable_value(
                        &mut app.wordlist,
                        Some(option.clone()),
                        option.as_ref(),
                    );
                }
            });
        if app.wordlist != before {
            app.update_wordlist();
        }
    });

    ui.horizontal(|ui| {
        if ui
            .add(
                Button::new("Run [space]")
                    .selected(app.run_state == RunState::Running),
            )
            .clicked()
        {
            app.toggle_running();
        }

        if ui
            .add(
                Button::new("Test [t]")
                    .selected(app.run_state == RunState::Test),
            )
            .clicked()
        {
            app.toggle_test_mode();
        }
    });

    if ui.button("Exit").clicked() {
        app.request_close.store(true, Ordering::Relaxed);
    }
}

pub fn window(
    ctx: &egui::Context,
    control_state: Arc<Mutex<crate::ControlState>>,
) {
    if ctx.input(|input| input.viewport().close_requested()) {
        control_state
            .lock()
            .unwrap()
            .request_close
            .store(true, Ordering::Relaxed);
    }

    let mut control_state = control_state.lock().unwrap();

    crate::input::process(ctx, control_state.deref_mut());

    egui::CentralPanel::default().show(ctx, |ui| {
        show(ui, control_state.deref_mut());
    });
}
