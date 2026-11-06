//! Application shell: screens, input handling, modals and persistence glue.

use crate::board::{self, BoardOptions, Flash};
use crate::db::{Db, GameSummary, MoveRow};
use crate::game::{Game, PlaceOutcome, Status, format_duration};
use crate::sudoku::{self, Difficulty, Puzzle};
use crate::theme::{self, Palette, bold, regular, semibold};
use crate::widgets::{self as w, Kind, lerp_color, with_alpha};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Event, Frame, Id, Key, Layout, Margin, Modal,
    Rect, Response, Sense, Stroke, StrokeKind, Ui, UiBuilder, pos2, vec2,
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Play = 0,
    History = 1,
    Stats = 2,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Confirm {
    Reveal,
    Restart,
    Delete(i64),
    ClearHistory,
}

struct Detail {
    game: Game,
    moves: Vec<MoveRow>,
    show_solution: bool,
}

struct Toast {
    text: String,
    at: Instant,
}

struct WinInfo {
    new_best: bool,
    prev_best: Option<f64>,
}

enum Act {
    Digit(u8, bool),
    Move(i32, i32),
    Erase,
    Notes,
    Hint,
    Undo,
    Redo,
    Pause,
    Deselect,
}

pub struct SudokuApp {
    db: Option<Db>,
    db_error: Option<String>,
    game: Game,
    selected: Option<usize>,
    notes_mode: bool,
    screen: Screen,
    dark: bool,
    applied_dark: Option<bool>,
    highlight_mistakes: bool,
    difficulty: Difficulty,
    paused: bool,
    show_help: bool,
    confirm: Option<Confirm>,
    win: Option<WinInfo>,
    flashes: Vec<Flash>,
    shake: Option<(usize, Instant)>,
    history: Vec<GameSummary>,
    history_dirty: bool,
    filter_diff: usize,
    filter_status: usize,
    detail: Option<Detail>,
    last_save: Instant,
    toast: Option<Toast>,
}

const DIFF_LABELS: [&str; 4] = ["Easy", "Medium", "Hard", "Expert"];

fn fmt_date(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.with_timezone(&chrono::Local).format("%b %-d, %Y · %-I:%M %p").to_string())
        .unwrap_or_default()
}

fn fmt_time_of_day(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.with_timezone(&chrono::Local).format("%-I:%M:%S %p").to_string())
        .unwrap_or_default()
}

fn cell_name(i: usize) -> String {
    format!("R{}C{}", i / 9 + 1, i % 9 + 1)
}

fn digit_of(k: Key) -> Option<u8> {
    Some(match k {
        Key::Num1 => 1,
        Key::Num2 => 2,
        Key::Num3 => 3,
        Key::Num4 => 4,
        Key::Num5 => 5,
        Key::Num6 => 6,
        Key::Num7 => 7,
        Key::Num8 => 8,
        Key::Num9 => 9,
        _ => return None,
    })
}

impl SudokuApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let (db, db_error) = match Db::open() {
            Ok(db) => (Some(db), None),
            Err(e) => (None, Some(e.to_string())),
        };
        let setting = |k: &str| db.as_ref().and_then(|d| d.get_setting(k));
        let dark = setting("dark")
            .map(|v| v == "1")
            .unwrap_or_else(|| cc.egui_ctx.system_theme() == Some(egui::Theme::Dark));
        let highlight_mistakes = setting("highlight_mistakes").map(|v| v == "1").unwrap_or(true);
        let difficulty = setting("difficulty")
            .map(|v| Difficulty::from_key(&v))
            .unwrap_or(Difficulty::Easy);

        let resumed = db
            .as_ref()
            .and_then(|d| d.latest_in_progress().ok().flatten())
            .and_then(|id| db.as_ref().and_then(|d| d.load_game(id).ok().flatten()));

        let resumed_flag = resumed.is_some();
        let game = resumed.unwrap_or_else(|| Game::new(difficulty, sudoku::generate(difficulty)));

        let mut app = SudokuApp {
            db,
            db_error,
            game,
            selected: None,
            notes_mode: false,
            screen: Screen::Play,
            dark,
            applied_dark: None,
            highlight_mistakes,
            difficulty,
            paused: false,
            show_help: false,
            confirm: None,
            win: None,
            flashes: Vec::new(),
            shake: None,
            history: Vec::new(),
            history_dirty: true,
            filter_diff: 0,
            filter_status: 0,
            detail: None,
            last_save: Instant::now(),
            toast: None,
        };
        app.persist();
        if resumed_flag {
            app.toast("Welcome back — your last game was restored");
        }
        if let Some(e) = &app.db_error {
            app.toast(&format!("History unavailable: {e}"));
        }
        app
    }

    // ------------------------------------------------------------ persistence

    fn persist(&mut self) {
        let entries = std::mem::take(&mut self.game.pending_log);
        if let Some(db) = &self.db {
            if self.game.id == 0 {
                if let Ok(id) = db.insert_game(&self.game) {
                    self.game.id = id;
                }
            } else {
                let _ = db.save_game(&self.game);
            }
            if self.game.id != 0 {
                let _ = db.log_moves(self.game.id, &entries);
            }
        }
        self.history_dirty = true;
        self.last_save = Instant::now();
    }

    fn refresh_history(&mut self) {
        if let Some(db) = &self.db {
            self.history = db.list_games().unwrap_or_default();
        }
        self.history_dirty = false;
    }

    fn set_setting(&self, k: &str, v: &str) {
        if let Some(db) = &self.db {
            db.set_setting(k, v);
        }
    }

    fn toast(&mut self, text: &str) {
        self.toast = Some(Toast {
            text: text.to_owned(),
            at: Instant::now(),
        });
    }

    // ---------------------------------------------------------- game control

    fn reset_view_state(&mut self) {
        self.selected = None;
        self.paused = false;
        self.notes_mode = false;
        self.flashes.clear();
        self.shake = None;
        self.win = None;
        self.screen = Screen::Play;
        self.detail = None;
    }

    fn start_fresh(&mut self, d: Difficulty) {
        self.game = Game::new(d, sudoku::generate(d));
        self.persist();
        self.reset_view_state();
        self.toast(&format!("New {} puzzle — good luck!", d.label()));
    }

    fn new_game(&mut self, d: Difficulty) {
        self.persist();
        self.start_fresh(d);
    }

    fn resume_game(&mut self, id: i64) {
        if id == self.game.id {
            self.reset_view_state();
            return;
        }
        self.persist();
        let loaded = self.db.as_ref().and_then(|db| db.load_game(id).ok().flatten());
        if let Some(g) = loaded {
            self.game = g;
            self.reset_view_state();
            self.toast("Game resumed");
        }
    }

    fn replay_game(&mut self, id: i64) {
        let loaded = self.db.as_ref().and_then(|db| db.load_game(id).ok().flatten());
        if let Some(g) = loaded {
            self.persist();
            self.game = Game::new(
                g.difficulty,
                Puzzle {
                    givens: g.givens,
                    solution: g.solution,
                },
            );
            self.persist();
            self.reset_view_state();
            self.toast("Replaying the same puzzle from scratch");
        }
    }

    fn open_detail(&mut self, id: i64) {
        if id == self.game.id {
            self.persist();
        }
        if let Some(db) = &self.db {
            if let Ok(Some(game)) = db.load_game(id) {
                let moves = db.moves_for(id).unwrap_or_default();
                self.detail = Some(Detail {
                    game,
                    moves,
                    show_solution: false,
                });
            }
        }
    }

    fn delete_game(&mut self, id: i64) {
        if let Some(db) = &self.db {
            let _ = db.delete_game(id);
        }
        if self.detail.as_ref().map(|d| d.game.id) == Some(id) {
            self.detail = None;
        }
        if id == self.game.id {
            let screen = self.screen;
            self.start_fresh(self.difficulty);
            self.screen = screen;
        }
        self.history_dirty = true;
        self.toast("Game deleted");
    }

    fn after_progress(&mut self, cell: usize, outcome: PlaceOutcome) {
        match outcome {
            PlaceOutcome::Solved => {
                self.flashes.push(Flash::wave(cell));
                self.on_solved();
            }
            PlaceOutcome::Correct => {
                for u in self.game.completed_units_at(cell) {
                    self.flashes.push(Flash::unit(&u));
                }
            }
            PlaceOutcome::Wrong => self.shake = Some((cell, Instant::now())),
            _ => {}
        }
    }

    fn on_solved(&mut self) {
        self.persist();
        self.refresh_history();
        let prev_best = self
            .history
            .iter()
            .filter(|g| g.status == Status::Completed && g.difficulty == self.game.difficulty && g.id != self.game.id)
            .map(|g| g.elapsed)
            .fold(None, |acc: Option<f64>, t| Some(acc.map_or(t, |a| a.min(t))));
        let elapsed = self.game.elapsed();
        self.win = Some(WinInfo {
            new_best: prev_best.is_none_or(|b| elapsed < b),
            prev_best,
        });
    }

    fn input_digit(&mut self, v: u8, as_note: bool) {
        if self.paused {
            return;
        }
        let Some(i) = self.selected else {
            self.toast("Select a cell first");
            return;
        };
        if !self.game.is_playing() {
            return;
        }
        if as_note || self.notes_mode {
            if self.game.cells[i] == 0 && !self.game.is_locked(i) {
                self.game.toggle_note(i, v);
            }
        } else {
            let outcome = self.game.place(i, v);
            self.after_progress(i, outcome);
        }
        self.persist();
    }

    fn erase(&mut self) {
        if let Some(i) = self.selected {
            if !self.paused {
                self.game.erase(i);
                self.persist();
            }
        }
    }

    fn undo(&mut self) {
        if self.paused {
            return;
        }
        if let Some(i) = self.game.undo() {
            self.selected = Some(i);
        }
        self.persist();
    }

    fn redo(&mut self) {
        if self.paused {
            return;
        }
        if let Some(i) = self.game.redo() {
            self.selected = Some(i);
            if self.game.status == Status::Completed {
                self.flashes.push(Flash::wave(i));
                self.on_solved();
            }
        }
        self.persist();
    }

    fn hint(&mut self) {
        if self.paused || !self.game.is_playing() {
            return;
        }
        if let Some(i) = self.game.hint(self.selected) {
            self.selected = Some(i);
            let outcome = if self.game.status == Status::Completed {
                PlaceOutcome::Solved
            } else {
                PlaceOutcome::Correct
            };
            self.after_progress(i, outcome);
            self.flashes.push(Flash::unit(&[i]));
        }
        self.persist();
    }

    fn modal_open(&self) -> bool {
        self.show_help || self.confirm.is_some() || self.win.is_some()
    }

    // ----------------------------------------------------------------- input

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if self.modal_open() || self.screen != Screen::Play {
            return;
        }
        let mut acts = Vec::new();
        ctx.input(|i| {
            for e in &i.events {
                if let Event::Key {
                    key,
                    physical_key,
                    pressed: true,
                    modifiers,
                    ..
                } = e
                {
                    let k = physical_key.unwrap_or(*key);
                    if modifiers.command {
                        match k {
                            Key::Z if modifiers.shift => acts.push(Act::Redo),
                            Key::Z => acts.push(Act::Undo),
                            Key::Y => acts.push(Act::Redo),
                            _ => {}
                        }
                        continue;
                    }
                    if let Some(d) = digit_of(k) {
                        acts.push(Act::Digit(d, modifiers.shift));
                        continue;
                    }
                    match k {
                        Key::ArrowUp | Key::W => acts.push(Act::Move(-1, 0)),
                        Key::ArrowDown | Key::S => acts.push(Act::Move(1, 0)),
                        Key::ArrowLeft | Key::A => acts.push(Act::Move(0, -1)),
                        Key::ArrowRight | Key::D => acts.push(Act::Move(0, 1)),
                        Key::Backspace | Key::Delete | Key::Num0 => acts.push(Act::Erase),
                        Key::N => acts.push(Act::Notes),
                        Key::H => acts.push(Act::Hint),
                        Key::Space | Key::P => acts.push(Act::Pause),
                        Key::Escape => acts.push(Act::Deselect),
                        _ => {}
                    }
                }
            }
        });
        for a in acts {
            match a {
                Act::Digit(d, note) => self.input_digit(d, note),
                Act::Move(dr, dc) => {
                    if self.paused {
                        continue;
                    }
                    let cur = self.selected.unwrap_or(40) as i32;
                    let (r, c) = (cur / 9, cur % 9);
                    let (r, c) = if self.selected.is_none() {
                        (r, c)
                    } else {
                        ((r + dr).rem_euclid(9), (c + dc).rem_euclid(9))
                    };
                    self.selected = Some((r * 9 + c) as usize);
                }
                Act::Erase => self.erase(),
                Act::Notes => self.notes_mode = !self.notes_mode,
                Act::Hint => self.hint(),
                Act::Undo => self.undo(),
                Act::Redo => self.redo(),
                Act::Pause => {
                    if self.game.is_playing() {
                        self.paused = !self.paused;
                    }
                }
                Act::Deselect => self.selected = None,
            }
        }
    }

    fn update_timer(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|i| i.focused);
        let run = self.screen == Screen::Play
            && !self.paused
            && focused
            && !self.modal_open()
            && self.game.is_playing();
        if run {
            self.game.resume_timer();
        } else if self.game.is_running() {
            self.game.pause_timer();
            self.persist();
        }
        if self.game.is_running() {
            ctx.request_repaint_after(Duration::from_millis(250));
            if self.last_save.elapsed() > Duration::from_secs(5) {
                self.persist();
            }
        }
    }

    // --------------------------------------------------------------- top bar

    fn top_bar(&mut self, ctx: &egui::Context, p: &Palette) {
        egui::TopBottomPanel::top("top_bar")
            .exact_height(64.0)
            .show_separator_line(false)
            .frame(Frame::new().fill(p.surface).inner_margin(Margin::symmetric(22, 0)))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                ui.painter().hline(
                    ctx.screen_rect().x_range(),
                    rect.bottom(),
                    Stroke::new(1.0, p.border),
                );

                ui.scope_builder(
                    UiBuilder::new().max_rect(rect).layout(Layout::left_to_right(Align::Center)),
                    |ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
                        draw_logo(ui, r, p);
                        ui.add_space(2.0);
                        ui.label(w::text("Sudoku", bold(19.0), p.text));
                    },
                );

                let tabs = Rect::from_center_size(rect.center(), vec2(372.0, 40.0));
                ui.scope_builder(UiBuilder::new().max_rect(tabs), |ui| {
                    let mut idx = self.screen as usize;
                    if w::segmented(ui, p, "nav", &["Play", "History", "Statistics"], &mut idx, 372.0, 40.0) {
                        self.screen = match idx {
                            0 => Screen::Play,
                            1 => Screen::History,
                            _ => Screen::Stats,
                        };
                        if self.screen != Screen::Play {
                            self.persist();
                            self.refresh_history();
                        }
                    }
                });

                ui.scope_builder(
                    UiBuilder::new().max_rect(rect).layout(Layout::right_to_left(Align::Center)),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let icon = if self.dark { "☀" } else { "🌙" };
                        let tip = if self.dark { "Switch to light mode" } else { "Switch to dark mode" };
                        if w::icon_button(ui, p, icon, tip).clicked() {
                            self.dark = !self.dark;
                            self.set_setting("dark", if self.dark { "1" } else { "0" });
                        }
                        if w::icon_button(ui, p, "❓", "How to play & shortcuts").clicked() {
                            self.show_help = true;
                        }
                    },
                );
            });
    }

    // ----------------------------------------------------------- play screen

    fn play_screen(&mut self, ui: &mut Ui, p: &Palette) {
        let avail = ui.max_rect();
        let panel_w = 356.0;
        let gap = 28.0;
        let side = avail
            .height()
            .min(avail.width() - panel_w - gap)
            .clamp(300.0, 820.0)
            .floor();
        let total = side + gap + panel_w;
        let left = (avail.center().x - total / 2.0).max(avail.left());
        let top = (avail.center().y - side / 2.0).max(avail.top());
        let board_rect = Rect::from_min_size(pos2(left, top), vec2(side, side));
        let panel_rect = Rect::from_min_max(
            pos2(board_rect.right() + gap - 12.0, avail.top()),
            pos2(board_rect.right() + gap + panel_w, avail.bottom()),
        );

        let hide = self.paused && self.game.is_playing();
        let opts = BoardOptions {
            selected: if hide { None } else { self.selected },
            highlight_mistakes: self.highlight_mistakes,
            flashes: &self.flashes,
            shake: self.shake,
            hide_digits: hide,
            interactive: !hide,
            show_solution: false,
            id: "main_board",
        };
        if let Some(c) = board::draw(ui, p, board_rect, &self.game, &opts) {
            self.selected = Some(c);
        }

        if hide {
            let overlay = Rect::from_center_size(board_rect.center(), vec2(260.0, 150.0));
            ui.painter().add(p.card_shadow().as_shape(overlay, CornerRadius::same(18)));
            ui.painter().rect_filled(overlay, CornerRadius::same(18), p.surface);
            ui.painter().rect_stroke(overlay, CornerRadius::same(18), Stroke::new(1.0, p.border), StrokeKind::Inside);
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(overlay.shrink(20.0))
                    .layout(Layout::top_down(Align::Center)),
                |ui| {
                    ui.add_space(4.0);
                    ui.label(w::text("Paused", bold(22.0), p.text));
                    ui.label(w::text("The timer is stopped", regular(13.0), p.text_muted));
                    ui.add_space(4.0);
                    if w::button(ui, p, "▶  Resume", Kind::Primary, 160.0, 40.0, true).clicked() {
                        self.paused = false;
                    }
                },
            );
        }

        ui.scope_builder(UiBuilder::new().max_rect(panel_rect), |ui| {
            egui::ScrollArea::vertical()
                .id_salt("side_panel")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    Frame::new()
                        .inner_margin(Margin {
                            left: 12,
                            right: 12,
                            top: (board_rect.top() - avail.top()) as i8 + 2,
                            bottom: 12,
                        })
                        .show(ui, |ui| {
                            ui.set_width(panel_w - 24.0);
                            self.side_panel(ui, p);
                        });
                });
        });
    }

    fn side_panel(&mut self, ui: &mut Ui, p: &Palette) {
        let width = ui.available_width();
        let g = &self.game;
        let playing = g.is_playing();

        // ---- Status card
        Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(CornerRadius::same(16))
            .inner_margin(Margin::same(18))
            .shadow(p.card_shadow_small())
            .show(ui, |ui| {
                ui.set_width(width - 38.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    w::badge(ui, g.difficulty.label(), p.difficulty_color(g.difficulty));
                    if !playing {
                        w::badge(ui, g.status.label(), p.status_color(g.status));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if playing {
                            let (icon, tip) = if self.paused { ("▶", "Resume (Space)") } else { ("⏸", "Pause (Space)") };
                            if w::icon_button(ui, p, icon, tip).clicked() {
                                self.paused = !self.paused;
                            }
                        }
                    });
                });
                let time_color = match g.status {
                    Status::Completed => p.success,
                    Status::Revealed => p.text_muted,
                    _ => p.text,
                };
                ui.label(w::text(&format_duration(g.elapsed()), bold(38.0), time_color));
                ui.add_space(-4.0);
                let sw = (ui.available_width() - 20.0) / 3.0;
                ui.horizontal(|ui| {
                    let mistakes_color = if g.mistakes > 0 { p.error } else { p.text };
                    w::stat(ui, p, "Mistakes", &g.mistakes.to_string(), mistakes_color, sw);
                    w::stat(ui, p, "Hints", &g.hints.to_string(), if g.hints > 0 { p.hinted } else { p.text }, sw);
                    let to_fill = (81 - g.givens_count()).max(1);
                    let done = g.filled_correct() - g.givens_count();
                    let pct = (done as f32 / to_fill as f32 * 100.0).round();
                    w::stat(ui, p, "Progress", &format!("{pct:.0}%"), p.text, sw);
                });
                let to_fill = (81 - g.givens_count()).max(1);
                let frac = (g.filled_correct() - g.givens_count()) as f32 / to_fill as f32;
                let bar_color = if g.status == Status::Completed { p.success } else { p.accent };
                w::progress_bar(ui, p, frac, ui.available_width(), bar_color);
            });

        ui.add_space(6.0);

        // ---- Tools
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let tw = (width - 4.0 * 8.0) / 5.0;
            let size = vec2(tw, 66.0);
            let can_edit = playing && !self.paused;
            if w::tool_button(ui, p, "⟲", "Undo", size, false, can_edit && self.game.can_undo(), None)
                .on_hover_text("Undo (Ctrl/⌘ + Z)")
                .clicked()
            {
                self.undo();
            }
            if w::tool_button(ui, p, "⟳", "Redo", size, false, can_edit && self.game.can_redo(), None)
                .on_hover_text("Redo (Ctrl/⌘ + Y)")
                .clicked()
            {
                self.redo();
            }
            if w::tool_button(ui, p, "⌫", "Erase", size, false, can_edit, None)
                .on_hover_text("Erase cell (Backspace)")
                .clicked()
            {
                self.erase();
            }
            let badge = if self.notes_mode { "ON" } else { "OFF" };
            if w::tool_button(ui, p, "✏", "Notes", size, self.notes_mode, can_edit, Some(badge))
                .on_hover_text("Pencil marks (N) — or hold Shift while typing a digit")
                .clicked()
            {
                self.notes_mode = !self.notes_mode;
            }
            if w::tool_button(ui, p, "💡", "Hint", size, false, can_edit, None)
                .on_hover_text("Reveal the selected cell, or the easiest next cell (H)")
                .clicked()
            {
                self.hint();
            }
        });

        ui.add_space(2.0);

        // ---- Number pad
        let kw = (width - 2.0 * 8.0) / 3.0;
        let kh = 62.0;
        for row in 0..3 {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                for col in 0..3 {
                    let d = (row * 3 + col + 1) as u8;
                    let remaining = 9usize.saturating_sub(self.game.count_of(d));
                    let enabled = playing && !self.paused;
                    if w::num_key(ui, p, d, remaining, self.notes_mode, vec2(kw, kh), enabled).clicked() {
                        self.input_digit(d, false);
                    }
                }
            });
            if row < 2 {
                ui.add_space(-2.0);
            }
        }

        ui.add_space(10.0);

        // ---- New game
        w::section_title(ui, p, "New game");
        ui.add_space(-4.0);
        let mut idx = Difficulty::ALL.iter().position(|&d| d == self.difficulty).unwrap_or(0);
        if w::segmented(ui, p, "difficulty", &DIFF_LABELS, &mut idx, width, 38.0) {
            self.difficulty = Difficulty::ALL[idx];
            self.set_setting("difficulty", self.difficulty.key());
        }
        let label = format!("✨  New {} Game", self.difficulty.label());
        if w::button(ui, p, &label, Kind::Primary, width, 44.0, true).clicked() {
            self.new_game(self.difficulty);
        }

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let bw = (width - 8.0) / 2.0;
            if w::button(ui, p, "↺  Restart", Kind::Secondary, bw, 40.0, playing).clicked() {
                self.confirm = Some(Confirm::Restart);
            }
            if w::button(ui, p, "👁  Show solution", Kind::DangerGhost, bw, 40.0, playing).clicked() {
                self.confirm = Some(Confirm::Reveal);
            }
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let mut on = self.highlight_mistakes;
            if toggle_switch(ui, p, &mut on).clicked() {
                self.highlight_mistakes = on;
                self.set_setting("highlight_mistakes", if on { "1" } else { "0" });
            }
            ui.label(w::text("Highlight mistakes", regular(13.5), p.text_muted));
        });
    }

    // -------------------------------------------------------- history screen

    fn history_screen(&mut self, ui: &mut Ui, p: &Palette) {
        if self.history_dirty {
            self.refresh_history();
        }
        if self.detail.is_some() {
            self.detail_view(ui, p);
            return;
        }
        let avail = ui.max_rect();
        let cw = avail.width().min(1060.0);
        let col = Rect::from_min_size(pos2(avail.center().x - cw / 2.0, avail.top()), vec2(cw, avail.height()));
        ui.scope_builder(UiBuilder::new().max_rect(col), |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(w::text("History", bold(28.0), p.text));
                    let n = self.history.len();
                    ui.label(w::text(
                        &format!("{n} game{} · saved locally in SQLite", if n == 1 { "" } else { "s" }),
                        regular(13.5),
                        p.text_muted,
                    ));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if w::button(ui, p, "🗑  Clear all", Kind::DangerGhost, 0.0, 38.0, !self.history.is_empty()).clicked() {
                        self.confirm = Some(Confirm::ClearHistory);
                    }
                });
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                w::segmented(ui, p, "hist_diff", &["All", "Easy", "Medium", "Hard", "Expert"], &mut self.filter_diff, 430.0, 38.0);
                ui.add_space(8.0);
                w::segmented(ui, p, "hist_status", &["All", "In progress", "Solved", "Revealed"], &mut self.filter_status, 400.0, 38.0);
            });
            ui.add_space(8.0);

            let diff_filter = self.filter_diff.checked_sub(1).map(|i| Difficulty::ALL[i]);
            let status_filter = match self.filter_status {
                1 => Some(Status::InProgress),
                2 => Some(Status::Completed),
                3 => Some(Status::Revealed),
                _ => None,
            };
            let rows: Vec<GameSummary> = self
                .history
                .iter()
                .filter(|g| diff_filter.is_none_or(|d| g.difficulty == d))
                .filter(|g| status_filter.is_none_or(|s| g.status == s))
                .cloned()
                .collect();

            if rows.is_empty() {
                ui.add_space(60.0);
                ui.vertical_centered(|ui| {
                    ui.label(w::text("📜", regular(44.0), p.text_faint));
                    ui.label(w::text("Nothing here yet", semibold(18.0), p.text));
                    ui.label(w::text("Games you play will show up here automatically.", regular(14.0), p.text_muted));
                    ui.add_space(8.0);
                    if w::button(ui, p, "Start playing", Kind::Primary, 180.0, 42.0, true).clicked() {
                        self.screen = Screen::Play;
                    }
                });
                return;
            }

            egui::ScrollArea::vertical()
                .id_salt("history_list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 10.0;
                    for g in &rows {
                        self.history_row(ui, p, g);
                    }
                    ui.add_space(12.0);
                });
        });
    }

    fn history_row(&mut self, ui: &mut Ui, p: &Palette, g: &GameSummary) {
        let is_current = g.id == self.game.id;
        let resp = Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, if is_current { with_alpha(p.accent, 0.5) } else { p.border }))
            .corner_radius(CornerRadius::same(14))
            .inner_margin(Margin::symmetric(18, 12))
            .outer_margin(Margin::symmetric(4, 0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    fixed(ui, 74.0, |ui| {
                        w::badge(ui, g.difficulty.label(), p.difficulty_color(g.difficulty));
                    });
                    fixed(ui, 200.0, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(w::text(&fmt_date(g.started_at), semibold(14.0), p.text));
                        let sub = if is_current { format!("Game #{} · current", g.id) } else { format!("Game #{}", g.id) };
                        ui.label(w::text(&sub, regular(12.0), p.text_faint));
                    });
                    fixed(ui, 92.0, |ui| {
                        w::badge(ui, g.status.label(), p.status_color(g.status));
                    });
                    w::stat(ui, p, "Time", &format_duration(g.elapsed), p.text, 72.0);
                    w::stat(ui, p, "Mistakes", &g.mistakes.to_string(), if g.mistakes > 0 { p.error } else { p.text }, 64.0);
                    w::stat(ui, p, "Hints", &g.hints.to_string(), p.text, 44.0);
                    fixed(ui, 90.0, |ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        ui.label(w::text(&format!("{:.0}%", g.progress * 100.0), semibold(13.0), p.text_muted));
                        let c = if g.status == Status::Completed { p.success } else { p.accent };
                        w::progress_bar(ui, p, g.progress, 86.0, c);
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        if w::icon_button(ui, p, "🗑", "Delete game").clicked() {
                            self.confirm = Some(Confirm::Delete(g.id));
                        }
                        if w::button(ui, p, "Replay", Kind::Secondary, 0.0, 36.0, true)
                            .on_hover_text("Play this exact puzzle again from scratch")
                            .clicked()
                        {
                            self.replay_game(g.id);
                        }
                        if g.status == Status::InProgress {
                            if w::button(ui, p, "Resume", Kind::Primary, 0.0, 36.0, true).clicked() {
                                self.resume_game(g.id);
                            }
                        } else if w::button(ui, p, "View", Kind::Secondary, 0.0, 36.0, true).clicked() {
                            self.open_detail(g.id);
                        }
                    });
                });
            })
            .response;
        // Clicking empty space on the row opens the details.
        let click = ui.interact(resp.rect, Id::new(("hist_row", g.id)), Sense::click());
        if click.hovered() {
            ui.painter().rect_stroke(
                resp.rect.shrink2(vec2(4.0, 0.0)),
                CornerRadius::same(14),
                Stroke::new(1.0, with_alpha(p.accent, 0.6)),
                StrokeKind::Inside,
            );
        }
        if click.clicked() {
            self.open_detail(g.id);
        }
    }

    fn detail_view(&mut self, ui: &mut Ui, p: &Palette) {
        let Some(detail) = &self.detail else { return };
        let avail = ui.max_rect();
        let cw = avail.width().min(1160.0);
        let col = Rect::from_min_size(pos2(avail.center().x - cw / 2.0, avail.top()), vec2(cw, avail.height()));
        let game = detail.game.clone();
        let show_solution = detail.show_solution;
        let mut action: Option<&'static str> = None;

        ui.scope_builder(UiBuilder::new().max_rect(col), |ui| {
            ui.horizontal(|ui| {
                if w::button(ui, p, "←  Back", Kind::Ghost, 0.0, 36.0, true).clicked() {
                    action = Some("back");
                }
                ui.label(w::text(&format!("Game #{}", game.id), bold(24.0), p.text));
                w::badge(ui, game.difficulty.label(), p.difficulty_color(game.difficulty));
                w::badge(ui, game.status.label(), p.status_color(game.status));
            });
            ui.add_space(8.0);

            let body = ui.available_rect_before_wrap();
            let side = (body.height() - 8.0).min(body.width() - 420.0).clamp(260.0, 680.0);
            let board_rect = Rect::from_min_size(body.min + vec2(8.0, 4.0), vec2(side, side));
            let opts = BoardOptions {
                selected: None,
                highlight_mistakes: true,
                flashes: &[],
                shake: None,
                hide_digits: false,
                interactive: false,
                show_solution,
                id: "detail_board",
            };
            board::draw(ui, p, board_rect, &game, &opts);

            let right = Rect::from_min_max(pos2(board_rect.right() + 28.0, body.top()), body.max);
            ui.scope_builder(UiBuilder::new().max_rect(right), |ui| {
                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(Margin::same(18))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let sw = (ui.available_width() - 30.0) / 4.0;
                            w::stat(ui, p, "Time", &format_duration(game.elapsed()), p.text, sw);
                            w::stat(ui, p, "Mistakes", &game.mistakes.to_string(), if game.mistakes > 0 { p.error } else { p.text }, sw);
                            w::stat(ui, p, "Hints", &game.hints.to_string(), p.text, sw);
                            w::stat(ui, p, "Moves", &game.moves.to_string(), p.text, sw);
                        });
                        ui.label(w::text(&format!("Started  {}", fmt_date(game.started_at)), regular(13.0), p.text_muted));
                        if let Some(f) = game.finished_at {
                            ui.label(w::text(&format!("Finished {}", fmt_date(f)), regular(13.0), p.text_muted));
                        }
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            if game.status == Status::InProgress {
                                if w::button(ui, p, "▶  Resume", Kind::Primary, 0.0, 38.0, true).clicked() {
                                    action = Some("resume");
                                }
                            }
                            if w::button(ui, p, "Replay puzzle", Kind::Secondary, 0.0, 38.0, true).clicked() {
                                action = Some("replay");
                            }
                            let sol = if show_solution { "Hide solution" } else { "Show solution" };
                            if w::button(ui, p, sol, Kind::Secondary, 0.0, 38.0, true).clicked() {
                                action = Some("toggle");
                            }
                            if w::button(ui, p, "Delete", Kind::DangerGhost, 0.0, 38.0, true).clicked() {
                                action = Some("delete");
                            }
                        });
                    });
                ui.add_space(6.0);
                ui.label(w::text("Move history", semibold(16.0), p.text));
                Frame::new()
                    .fill(p.surface)
                    .stroke(Stroke::new(1.0, p.border))
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(Margin::symmetric(6, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::ScrollArea::vertical()
                            .id_salt("moves")
                            .auto_shrink([false, false])
                            .max_height(ui.available_height() - 16.0)
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                if detail.moves.is_empty() {
                                    ui.label(w::text("No moves recorded.", regular(13.0), p.text_muted));
                                }
                                for (k, m) in detail.moves.iter().enumerate() {
                                    move_row(ui, p, k + 1, m);
                                }
                            });
                    });
            });
        });

        match action {
            Some("back") => self.detail = None,
            Some("resume") => self.resume_game(game.id),
            Some("replay") => self.replay_game(game.id),
            Some("toggle") => {
                if let Some(d) = &mut self.detail {
                    d.show_solution = !d.show_solution;
                }
            }
            Some("delete") => self.confirm = Some(Confirm::Delete(game.id)),
            _ => {}
        }
    }

    // ---------------------------------------------------------- stats screen

    fn stats_screen(&mut self, ui: &mut Ui, p: &Palette) {
        if self.history_dirty {
            self.refresh_history();
        }
        let avail = ui.max_rect();
        let cw = avail.width().min(1100.0);
        let col = Rect::from_min_size(pos2(avail.center().x - cw / 2.0, avail.top()), vec2(cw, avail.height()));
        let h = &self.history;

        let completed = h.iter().filter(|g| g.status == Status::Completed).count();
        let revealed = h.iter().filter(|g| g.status == Status::Revealed).count();
        let finished = completed + revealed;
        let total_time: f64 = h.iter().map(|g| g.elapsed).sum();

        // Streaks: chronological order, in-progress games are ignored.
        let mut chrono_games: Vec<&GameSummary> = h.iter().filter(|g| g.status != Status::InProgress).collect();
        chrono_games.sort_by_key(|g| (g.updated_at, g.id));
        let (mut best_streak, mut run) = (0, 0);
        for g in &chrono_games {
            if g.status == Status::Completed {
                run += 1;
                best_streak = best_streak.max(run);
            } else {
                run = 0;
            }
        }
        let current_streak = run;

        ui.scope_builder(UiBuilder::new().max_rect(col), |ui| {
            egui::ScrollArea::vertical()
                .id_salt("stats")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(cw - 10.0);
                    ui.label(w::text("Statistics", bold(28.0), p.text));
                    ui.label(w::text("Your progress across every game you've played", regular(13.5), p.text_muted));
                    ui.add_space(6.0);

                    let n = 6.0;
                    let gap = 12.0;
                    let card_w = (cw - 10.0 - 8.0 - gap * (n - 1.0)) / n;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        ui.add_space(4.0);
                        let win_rate = if finished > 0 { format!("{:.0}%", completed as f64 / finished as f64 * 100.0) } else { "—".into() };
                        let items = [
                            ("🎮", "Played", h.len().to_string(), p.accent),
                            ("🏆", "Solved", completed.to_string(), p.success),
                            ("📈", "Win rate", win_rate, p.accent),
                            ("🔥", "Streak", current_streak.to_string(), p.warning),
                            ("⭐", "Best streak", best_streak.to_string(), p.warning),
                            ("⏱", "Time played", format_duration(total_time), p.revealed),
                        ];
                        for (icon, label, value, color) in items {
                            overview_card(ui, p, card_w, icon, label, &value, color);
                        }
                    });

                    ui.add_space(14.0);
                    ui.label(w::text("By difficulty", semibold(17.0), p.text));
                    let card_w = (cw - 10.0 - 8.0 - gap * 3.0) / 4.0;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        ui.add_space(4.0);
                        for d in Difficulty::ALL {
                            difficulty_card(ui, p, card_w, d, h);
                        }
                    });

                    ui.add_space(14.0);
                    ui.label(w::text("Recent solve times", semibold(17.0), p.text));
                    let mut recent: Vec<&GameSummary> = h.iter().filter(|g| g.status == Status::Completed).take(20).collect();
                    recent.reverse();
                    Frame::new()
                        .fill(p.surface)
                        .stroke(Stroke::new(1.0, p.border))
                        .corner_radius(CornerRadius::same(16))
                        .inner_margin(Margin::same(18))
                        .outer_margin(Margin { left: 4, right: 4, top: 0, bottom: 16 })
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            solve_chart(ui, p, &recent);
                        });
                });
        });
    }

    // ---------------------------------------------------------------- modals

    fn modals(&mut self, ctx: &egui::Context, p: &Palette) {
        let frame = Frame::new()
            .fill(p.surface)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(CornerRadius::same(20))
            .inner_margin(Margin::same(28))
            .shadow(egui::epaint::Shadow {
                offset: [0, 16],
                blur: 48,
                spread: 0,
                color: p.shadow,
            });
        let backdrop = Color32::from_black_alpha(if p.dark { 150 } else { 90 });

        if self.show_help {
            let mut close = false;
            let r = Modal::new(Id::new("help"))
                .frame(frame)
                .backdrop_color(backdrop)
                .show(ctx, |ui| {
                    ui.set_width(560.0);
                    close = help_content(ui, p);
                });
            if close || r.should_close() {
                self.show_help = false;
            }
        }

        if let Some(c) = self.confirm {
            let (icon, title, body, confirm_label, danger) = match c {
                Confirm::Reveal => (
                    "👁",
                    "Show the full solution?",
                    "This ends the current game. It will be saved in your history as “Revealed”, and cells you hadn't solved will be shown in a different colour.",
                    "Show solution",
                    true,
                ),
                Confirm::Restart => (
                    "↺",
                    "Restart this puzzle?",
                    "All your entries, notes, mistakes, hints and the timer will be reset.",
                    "Restart",
                    true,
                ),
                Confirm::Delete(_) => (
                    "🗑",
                    "Delete this game?",
                    "The game and its full move history will be permanently removed.",
                    "Delete",
                    true,
                ),
                Confirm::ClearHistory => (
                    "🗑",
                    "Clear all history?",
                    "Every game, statistic and move log will be permanently deleted. This cannot be undone.",
                    "Clear everything",
                    true,
                ),
            };
            let mut result: Option<bool> = None;
            let r = Modal::new(Id::new("confirm"))
                .frame(frame)
                .backdrop_color(backdrop)
                .show(ctx, |ui| {
                    ui.set_width(400.0);
                    let (cr, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
                    let color = if danger { p.error } else { p.accent };
                    ui.painter().circle_filled(cr.center(), 24.0, with_alpha(color, 0.12));
                    ui.painter().text(cr.center(), Align2::CENTER_CENTER, icon, regular(22.0), color);
                    ui.label(w::text(title, bold(20.0), p.text));
                    ui.label(w::text(body, regular(14.0), p.text_muted));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let bw = (ui.available_width() - 10.0) / 2.0;
                        if w::button(ui, p, "Cancel", Kind::Secondary, bw, 42.0, true).clicked() {
                            result = Some(false);
                        }
                        let kind = if danger { Kind::Danger } else { Kind::Primary };
                        if w::button(ui, p, confirm_label, kind, bw, 42.0, true).clicked() {
                            result = Some(true);
                        }
                    });
                });
            if r.should_close() && result.is_none() {
                result = Some(false);
            }
            if let Some(ok) = result {
                self.confirm = None;
                if ok {
                    match c {
                        Confirm::Reveal => {
                            self.game.reveal();
                            self.selected = None;
                            self.paused = false;
                            self.persist();
                            self.toast("Solution revealed — saved to history");
                        }
                        Confirm::Restart => {
                            self.game.restart();
                            self.selected = None;
                            self.flashes.clear();
                            self.persist();
                            self.toast("Puzzle restarted");
                        }
                        Confirm::Delete(id) => self.delete_game(id),
                        Confirm::ClearHistory => {
                            if let Some(db) = &self.db {
                                let _ = db.clear_history();
                            }
                            self.game.pending_log.clear();
                            let screen = self.screen;
                            self.start_fresh(self.difficulty);
                            self.screen = screen;
                            self.refresh_history();
                            self.toast("History cleared");
                        }
                    }
                }
            }
        }

        if let Some(info) = &self.win {
            let mut choice: Option<u8> = None;
            let g = &self.game;
            let r = Modal::new(Id::new("win"))
                .frame(frame)
                .backdrop_color(backdrop)
                .show(ctx, |ui| {
                    ui.set_width(420.0);
                    ui.vertical_centered(|ui| {
                        let t = ui.ctx().animate_bool_with_time(Id::new("win_pop"), true, 0.35);
                        let (cr, _) = ui.allocate_exact_size(vec2(84.0, 84.0), Sense::hover());
                        ui.painter().circle_filled(cr.center(), 42.0 * t, p.success_soft);
                        ui.painter().text(cr.center(), Align2::CENTER_CENTER, "🏆", regular(40.0 * t.max(0.1)), p.success);
                        ui.label(w::text("Puzzle solved!", bold(26.0), p.text));
                        ui.label(w::text(
                            &format!("{} · {}", g.difficulty.label(), fmt_date(g.finished_at.unwrap_or(g.started_at))),
                            regular(13.5),
                            p.text_muted,
                        ));
                        if info.new_best {
                            ui.add_space(2.0);
                            let label = if info.prev_best.is_some() { "★ New personal best!" } else { "★ First solve at this level!" };
                            w::badge(ui, label, p.warning);
                        }
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let sw = (ui.available_width() - 20.0) / 3.0;
                        w::stat(ui, p, "Time", &format_duration(g.elapsed()), p.text, sw);
                        w::stat(ui, p, "Mistakes", &g.mistakes.to_string(), if g.mistakes > 0 { p.error } else { p.success }, sw);
                        w::stat(ui, p, "Hints", &g.hints.to_string(), p.text, sw);
                    });
                    if let Some(b) = info.prev_best {
                        ui.label(w::text(&format!("Previous best: {}", format_duration(b)), regular(12.5), p.text_faint));
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let bw = (ui.available_width() - 10.0) / 2.0;
                        if w::button(ui, p, "Close", Kind::Secondary, bw, 44.0, true).clicked() {
                            choice = Some(0);
                        }
                        if w::button(ui, p, &format!("New {} game", self.difficulty.label()), Kind::Primary, bw, 44.0, true).clicked() {
                            choice = Some(1);
                        }
                    });
                });
            if r.should_close() && choice.is_none() {
                choice = Some(0);
            }
            match choice {
                Some(0) => self.win = None,
                Some(_) => {
                    self.win = None;
                    self.new_game(self.difficulty);
                }
                None => {}
            }
        }
    }

    fn toast_ui(&mut self, ctx: &egui::Context, p: &Palette) {
        let Some(t) = &self.toast else { return };
        let age = t.at.elapsed().as_secs_f32();
        let life = 2.6;
        if age > life {
            self.toast = None;
            return;
        }
        let alpha = (age / 0.18).min(1.0).min((life - age) / 0.35);
        let rise = (1.0 - (age / 0.25).min(1.0)) * 10.0;
        egui::Area::new(Id::new("toast"))
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -28.0 + rise))
            .order(egui::Order::Tooltip)
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_opacity(alpha.clamp(0.0, 1.0));
                Frame::new()
                    .fill(if p.dark { p.surface_alt } else { p.given })
                    .stroke(Stroke::new(1.0, if p.dark { p.border } else { p.given }))
                    .corner_radius(CornerRadius::same(22))
                    .inner_margin(Margin::symmetric(20, 11))
                    .shadow(p.card_shadow())
                    .show(ui, |ui| {
                        ui.label(w::text(&t.text, semibold(13.5), if p.dark { p.text } else { Color32::WHITE }));
                    });
            });
        ctx.request_repaint();
    }
}

impl eframe::App for SudokuApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let p = Palette::get(self.dark);
        if self.applied_dark != Some(self.dark) {
            ctx.set_theme(if self.dark { egui::Theme::Dark } else { egui::Theme::Light });
            theme::apply_style(ctx, &p);
            self.applied_dark = Some(self.dark);
        }

        self.update_timer(ctx);
        self.handle_keys(ctx);
        self.top_bar(ctx, &p);

        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg).inner_margin(Margin::same(26)))
            .show(ctx, |ui| match self.screen {
                Screen::Play => self.play_screen(ui, &p),
                Screen::History => self.history_screen(ui, &p),
                Screen::Stats => self.stats_screen(ui, &p),
            });

        self.modals(ctx, &p);
        self.toast_ui(ctx, &p);

        self.flashes.retain(|f| !f.finished());
        if let Some((_, t)) = self.shake {
            if t.elapsed().as_secs_f32() > 0.45 {
                self.shake = None;
            }
        }
        if !self.flashes.is_empty() || self.shake.is_some() {
            ctx.request_repaint();
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.game.pause_timer();
        self.persist();
    }
}

impl Drop for SudokuApp {
    fn drop(&mut self) {
        self.game.pause_timer();
        self.persist();
    }
}

// -------------------------------------------------------------------- helpers

fn fixed(ui: &mut Ui, width: f32, add: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(vec2(width, 44.0), Layout::top_down(Align::Min), |ui| {
        ui.set_width(width);
        add(ui);
    });
}

fn draw_logo(ui: &Ui, r: Rect, p: &Palette) {
    let painter = ui.painter();
    let s = r.width() / 3.0;
    for k in 0..9 {
        let cell = Rect::from_min_size(
            pos2(r.left() + (k % 3) as f32 * s, r.top() + (k / 3) as f32 * s),
            vec2(s, s),
        )
        .shrink(1.3);
        let color = if k == 4 || k == 0 || k == 8 { p.accent } else { with_alpha(p.accent, 0.25) };
        painter.rect_filled(cell, CornerRadius::same(3), color);
    }
}

fn toggle_switch(ui: &mut Ui, p: &Palette, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(40.0, 22.0), Sense::click());
    let resp_c = resp.clone();
    if resp_c.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.15);
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(11), lerp_color(p.border, p.accent, t));
    let x = egui::lerp(rect.left() + 11.0..=rect.right() - 11.0, t);
    painter.circle_filled(pos2(x, rect.center().y), 8.0, Color32::WHITE);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn key_chip(ui: &mut Ui, p: &Palette, text: &str) {
    Frame::new()
        .fill(p.surface_alt)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(w::text(text, semibold(12.0), p.text));
        });
}

/// Returns true when the user clicked "Got it".
fn help_content(ui: &mut Ui, p: &Palette) -> bool {
    ui.label(w::text("How to play", bold(24.0), p.text));
    ui.label(w::text(
        "Fill every row, column and 3×3 box with the digits 1–9 — each digit exactly once.",
        regular(14.0),
        p.text_muted,
    ));
    ui.add_space(6.0);
    w::section_title(ui, p, "Controls");
    let rows: [(&str, &str); 10] = [
        ("Click / ← ↑ → ↓", "Select a cell"),
        ("1 – 9", "Place a digit (press again to clear)"),
        ("Shift + 1 – 9", "Toggle a pencil note"),
        ("N", "Toggle notes mode"),
        ("Backspace / Del", "Erase the cell"),
        ("H", "Hint — reveal the selected / easiest cell"),
        ("Ctrl/⌘ + Z", "Undo"),
        ("Ctrl/⌘ + Y", "Redo"),
        ("Space", "Pause / resume"),
        ("Esc", "Deselect"),
    ];
    egui::Grid::new("keys").num_columns(2).spacing(vec2(16.0, 6.0)).show(ui, |ui| {
        for (k, d) in rows {
            key_chip(ui, p, k);
            ui.label(w::text(d, regular(13.5), p.text));
            ui.end_row();
        }
    });
    ui.add_space(6.0);
    w::section_title(ui, p, "Colours");
    let legend = [
        (p.given, "Given digit"),
        (p.user, "Your digit"),
        (p.error, "Mistake"),
        (p.hinted, "Hint"),
        (p.revealed, "Revealed"),
    ];
    ui.horizontal_wrapped(|ui| {
        for (c, label) in legend {
            let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 6.0, c);
            ui.label(w::text(label, regular(13.0), p.text_muted));
            ui.add_space(6.0);
        }
    });
    ui.add_space(4.0);
    ui.label(w::text(
        "Tip: every game is saved automatically — close the app any time and pick up where you left off.",
        regular(12.5),
        p.text_faint,
    ));
    ui.add_space(8.0);
    w::button(ui, p, "Got it", Kind::Primary, ui.available_width(), 42.0, true).clicked()
}

fn move_row(ui: &mut Ui, p: &Palette, n: usize, m: &MoveRow) {
    let cell = m.cell.map(cell_name).unwrap_or_default();
    let (icon, text, color) = match m.action.as_str() {
        "start" => ("●", "Game started".to_string(), p.accent),
        "place" if m.correct == Some(false) => ("✗", format!("Placed {} at {cell}", m.value), p.error),
        "place" => ("✓", format!("Placed {} at {cell}", m.value), p.success),
        "erase" => ("⌫", format!("Erased {} at {cell}", m.value), p.text_muted),
        "note" => ("✏", format!("Note {} at {cell}", m.value), p.text_muted),
        "hint" => ("💡", format!("Hint: {} at {cell}", m.value), p.hinted),
        "undo" => ("⟲", "Undo".to_string(), p.text_muted),
        "redo" => ("⟳", "Redo".to_string(), p.text_muted),
        "reveal" => ("👁", "Solution revealed".to_string(), p.revealed),
        "restart" => ("↺", "Restarted".to_string(), p.warning),
        "solved" => ("🏆", "Puzzle solved".to_string(), p.success),
        other => ("•", other.to_string(), p.text_muted),
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(rect, CornerRadius::same(8), p.cell_hover);
    } else if n % 2 == 0 {
        painter.rect_filled(rect, CornerRadius::same(8), with_alpha(p.surface_alt, 0.6));
    }
    painter.text(pos2(rect.left() + 10.0, rect.center().y), Align2::LEFT_CENTER, format!("{n}"), regular(11.5), p.text_faint);
    painter.text(pos2(rect.left() + 50.0, rect.center().y), Align2::CENTER_CENTER, icon, regular(14.0), color);
    painter.text(pos2(rect.left() + 70.0, rect.center().y), Align2::LEFT_CENTER, text, regular(13.5), p.text);
    painter.text(pos2(rect.right() - 10.0, rect.center().y), Align2::RIGHT_CENTER, fmt_time_of_day(m.at), regular(11.5), p.text_faint);
    let _ = m.seq;
}

fn overview_card(ui: &mut Ui, p: &Palette, width: f32, icon: &str, label: &str, value: &str, color: Color32) {
    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::same(16))
        .shadow(p.card_shadow_small())
        .show(ui, |ui| {
            ui.set_width(width - 34.0);
            ui.spacing_mut().item_spacing.y = 4.0;
            let (r, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(10), with_alpha(color, 0.13));
            ui.painter().text(r.center(), Align2::CENTER_CENTER, icon, regular(17.0), color);
            ui.add_space(4.0);
            ui.label(w::text(value, bold(24.0), p.text));
            ui.label(w::text(label, regular(12.5), p.text_muted));
        });
}

fn difficulty_card(ui: &mut Ui, p: &Palette, width: f32, d: Difficulty, h: &[GameSummary]) {
    let games: Vec<&GameSummary> = h.iter().filter(|g| g.difficulty == d).collect();
    let solved: Vec<&&GameSummary> = games.iter().filter(|g| g.status == Status::Completed).collect();
    let revealed = games.iter().filter(|g| g.status == Status::Revealed).count();
    let best = solved.iter().map(|g| g.elapsed).fold(None, |a: Option<f64>, t| Some(a.map_or(t, |a| a.min(t))));
    let avg = if solved.is_empty() { None } else { Some(solved.iter().map(|g| g.elapsed).sum::<f64>() / solved.len() as f64) };
    let avg_mistakes = if solved.is_empty() { None } else { Some(solved.iter().map(|g| g.mistakes as f64).sum::<f64>() / solved.len() as f64) };
    let finished = solved.len() + revealed;
    let color = p.difficulty_color(d);

    Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(Margin::same(18))
        .shadow(p.card_shadow_small())
        .show(ui, |ui| {
            ui.set_width(width - 38.0);
            ui.horizontal(|ui| {
                w::badge(ui, d.label(), color);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(w::text(&format!("{} played", games.len()), regular(12.5), p.text_faint));
                });
            });
            ui.add_space(2.0);
            ui.label(w::text("BEST TIME", semibold(10.5), p.text_faint));
            ui.label(w::text(&best.map(format_duration).unwrap_or_else(|| "—".into()), bold(28.0), if best.is_some() { color } else { p.text_faint }));
            ui.add_space(2.0);
            let line = |ui: &mut Ui, k: &str, v: String| {
                ui.horizontal(|ui| {
                    ui.label(w::text(k, regular(13.0), p.text_muted));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(w::text(&v, semibold(13.0), p.text));
                    });
                });
            };
            line(ui, "Solved", solved.len().to_string());
            line(
                ui,
                "Win rate",
                if finished > 0 { format!("{:.0}%", solved.len() as f64 / finished as f64 * 100.0) } else { "—".into() },
            );
            line(ui, "Average time", avg.map(format_duration).unwrap_or_else(|| "—".into()));
            line(ui, "Avg. mistakes", avg_mistakes.map(|m| format!("{m:.1}")).unwrap_or_else(|| "—".into()));
        });
}

fn solve_chart(ui: &mut Ui, p: &Palette, games: &[&GameSummary]) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 190.0), Sense::hover());
    let painter = ui.painter();
    if games.is_empty() {
        painter.text(rect.center(), Align2::CENTER_CENTER, "Solve a puzzle to see your times here", regular(14.0), p.text_faint);
        return;
    }
    let plot = Rect::from_min_max(rect.min + vec2(48.0, 6.0), rect.max - vec2(0.0, 22.0));
    let max_t = games.iter().map(|g| g.elapsed).fold(60.0, f64::max) * 1.1;
    for k in 0..=4 {
        let y = plot.bottom() - plot.height() * k as f32 / 4.0;
        painter.hline(plot.x_range(), y, Stroke::new(1.0, with_alpha(p.line_thin, if k == 0 { 1.0 } else { 0.6 })));
        painter.text(
            pos2(plot.left() - 8.0, y),
            Align2::RIGHT_CENTER,
            format_duration(max_t * k as f64 / 4.0),
            regular(11.0),
            p.text_faint,
        );
    }
    let n = games.len().max(8) as f32;
    let slot = plot.width() / n;
    let bw = (slot * 0.56).min(36.0);
    let hover = ui.input(|i| i.pointer.hover_pos());
    for (k, g) in games.iter().enumerate() {
        let h = (g.elapsed / max_t) as f32 * plot.height();
        let x = plot.left() + slot * (k as f32 + 0.5);
        let anim = ui.ctx().animate_value_with_time(Id::new(("bar", g.id)), h, 0.4);
        let bar = Rect::from_min_max(pos2(x - bw / 2.0, plot.bottom() - anim), pos2(x + bw / 2.0, plot.bottom()));
        let color = p.difficulty_color(g.difficulty);
        let hovered = hover.is_some_and(|h| bar.expand2(vec2(slot * 0.2, 0.0)).contains(h) || (h.x - x).abs() < slot / 2.0 && plot.contains(h));
        painter.rect_filled(
            bar,
            CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 },
            if hovered { color } else { with_alpha(color, 0.75) },
        );
        painter.text(pos2(x, plot.bottom() + 12.0), Align2::CENTER_CENTER, format!("#{}", g.id), regular(10.5), p.text_faint);
        if hovered {
            let tip = format!("{} · {} · {} mistake{}", g.difficulty.label(), format_duration(g.elapsed), g.mistakes, if g.mistakes == 1 { "" } else { "s" });
            let galley = painter.layout_no_wrap(tip, semibold(12.0), p.text);
            let tr = Rect::from_center_size(pos2(x, bar.top() - 18.0), galley.size() + vec2(16.0, 10.0));
            let tr = tr.translate(vec2((rect.right() - tr.right()).min(0.0) + (rect.left() - tr.left()).max(0.0), 0.0));
            painter.rect_filled(tr, CornerRadius::same(8), p.surface_alt);
            painter.rect_stroke(tr, CornerRadius::same(8), Stroke::new(1.0, p.border), StrokeKind::Inside);
            painter.galley(tr.min + vec2(8.0, 5.0), galley, p.text);
        }
    }
}
