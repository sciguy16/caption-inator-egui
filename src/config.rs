use crate::{
    DisplayMode, MAX_FONT, MAX_SUBTITLE_HEIGHT, MIN_FONT, MIN_SUBTITLE_HEIGHT,
};
use egui::{ComboBox, RichText, Slider, Ui};

pub fn show(ui: &mut Ui, app: &mut crate::MyApp) {
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

    ui.checkbox(&mut app.dark_mode_requested, "Dark Mode");

    ui.horizontal(|ui| {
        ui.label("Display mode");
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
}
