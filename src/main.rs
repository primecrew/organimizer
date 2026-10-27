// No console window on Windows (release and debug alike).
#![windows_subsystem = "windows"]

mod gui;

fn main() -> eframe::Result {
    let loaded = photo_sorter::settings::load_or_create(photo_sorter::settings::ini_path());
    let s = &loaded.settings;
    // Remembered position / size / maximized, clamped to the monitors that
    // exist now (off-screen or tiny -> default size, centered).
    let saved_pos = s.layout.x.zip(s.layout.y).map(|(x, y)| [x, y]);
    let place = photo_sorter::window_state::restore_placement(
        saved_pos,
        [s.window_width, s.window_height],
        s.layout.maximized,
        &photo_sorter::desktop::monitor_work_areas(),
    );
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("PhotoSorter")
        .with_inner_size(place.size)
        .with_min_inner_size(photo_sorter::window_state::MIN_SIZE)
        .with_maximized(place.maximized)
        .with_drag_and_drop(true)
        .with_icon(gui::app_icon());
    if let Some(pos) = place.pos {
        viewport = viewport.with_position(pos);
    }
    let options = eframe::NativeOptions { viewport, centered: place.pos.is_none(), ..Default::default() };
    eframe::run_native("PhotoSorter", options, Box::new(move |cc| Ok(Box::new(gui::App::new(cc, loaded)))))
}
