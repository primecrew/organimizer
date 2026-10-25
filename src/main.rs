// No console window on Windows (release and debug alike).
#![windows_subsystem = "windows"]

mod gui;

fn main() -> eframe::Result {
    let loaded = photo_sorter::settings::load_or_create(photo_sorter::settings::ini_path());
    let s = &loaded.settings;
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_title("PhotoSorter")
        .with_inner_size([s.window_width, s.window_height])
        .with_min_inner_size([760.0, 480.0])
        .with_drag_and_drop(true)
        .with_icon(gui::app_icon());
    let options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    eframe::run_native("PhotoSorter", options, Box::new(move |cc| Ok(Box::new(gui::App::new(cc, loaded)))))
}
