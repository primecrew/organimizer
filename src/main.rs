// No console window on Windows (release and debug alike).
#![windows_subsystem = "windows"]

mod gui;

fn main() -> eframe::Result {
    // first start of 3.0: imports PhotoSorter 2.x settings (read only)
    let (ini, legacy) = organimizer::settings::ini_path_and_legacy();
    let loaded = organimizer::settings::load_or_import(ini, &legacy);
    let s = &loaded.settings;
    // Remembered position / size / maximized, clamped to the monitors that
    // exist now (off-screen or tiny -> default size, centered).
    let saved_pos = s.layout.x.zip(s.layout.y).map(|(x, y)| [x, y]);
    let place = organimizer::window_state::restore_placement(
        saved_pos,
        [s.window_width, s.window_height],
        s.layout.maximized,
        &organimizer::desktop::monitor_work_areas(),
    );
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title(organimizer::about::PRODUCT)
        .with_inner_size(place.size)
        .with_min_inner_size(organimizer::window_state::MIN_SIZE)
        .with_maximized(place.maximized)
        .with_drag_and_drop(true)
        .with_icon(gui::app_icon());
    if let Some(pos) = place.pos {
        viewport = viewport.with_position(pos);
    }
    let options = eframe::NativeOptions { viewport, centered: place.pos.is_none(), ..Default::default() };
    eframe::run_native("Organimizer", options, Box::new(move |cc| Ok(Box::new(gui::App::new(cc, loaded)))))
}
