use std::path::Path;

pub fn show(ctx: &egui::Context, images_dir: &Path, image: &str) {
    let image_path = images_dir.join(image);

    let image_uri = format!("file://{}", image_path.display());
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add(egui::Image::new(image_uri));
    });
}
