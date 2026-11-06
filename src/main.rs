#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod board;
mod db;
mod game;
mod sudoku;
mod theme;
mod widgets;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Sudoku")
            .with_app_id("sudoku")
            .with_inner_size([1240.0, 860.0])
            .with_min_inner_size([1000.0, 720.0]),
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "Sudoku",
        options,
        Box::new(|cc| Ok(Box::new(app::SudokuApp::new(cc)))),
    )
}
