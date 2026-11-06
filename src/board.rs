//! Painting and hit-testing of the 9×9 board.

use crate::game::Game;
use crate::sudoku::{box_of, col_of, row_of};
use crate::theme::{Palette, regular, semibold};
use crate::widgets::{lerp_color, with_alpha};
use eframe::egui::{
    self, Align2, CornerRadius, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use std::time::Instant;

/// A short highlight animation over a set of cells (completed unit / solve wave).
pub struct Flash {
    pub cells: Vec<(usize, f32)>, // (cell, delay in seconds)
    pub start: Instant,
    pub duration: f32,
}

impl Flash {
    pub fn unit(cells: &[usize]) -> Self {
        Flash {
            cells: cells.iter().enumerate().map(|(k, &c)| (c, k as f32 * 0.025)).collect(),
            start: Instant::now(),
            duration: 0.55,
        }
    }

    /// A ripple radiating out from `origin` across the whole board.
    pub fn wave(origin: usize) -> Self {
        let (r0, c0) = (row_of(origin) as f32, col_of(origin) as f32);
        Flash {
            cells: (0..81)
                .map(|i| {
                    let d = ((row_of(i) as f32 - r0).powi(2) + (col_of(i) as f32 - c0).powi(2)).sqrt();
                    (i, d * 0.045)
                })
                .collect(),
            start: Instant::now(),
            duration: 0.7,
        }
    }

    pub fn finished(&self) -> bool {
        let max_delay = self.cells.iter().map(|c| c.1).fold(0.0, f32::max);
        self.start.elapsed().as_secs_f32() > max_delay + self.duration
    }

    fn intensity(&self, cell: usize) -> f32 {
        let t = self.start.elapsed().as_secs_f32();
        self.cells
            .iter()
            .filter(|(c, _)| *c == cell)
            .map(|(_, delay)| {
                let local = (t - delay) / self.duration;
                if (0.0..1.0).contains(&local) {
                    (local * std::f32::consts::PI).sin()
                } else {
                    0.0
                }
            })
            .fold(0.0, f32::max)
    }
}

pub struct BoardOptions<'a> {
    pub selected: Option<usize>,
    pub highlight_mistakes: bool,
    pub flashes: &'a [Flash],
    pub shake: Option<(usize, Instant)>,
    pub hide_digits: bool,
    pub interactive: bool,
    pub show_solution: bool,
    pub id: &'a str,
}

const RADIUS: u8 = 14;

pub fn cell_rect(board: Rect, i: usize) -> Rect {
    let cs = board.width() / 9.0;
    Rect::from_min_size(
        pos2(board.left() + col_of(i) as f32 * cs, board.top() + row_of(i) as f32 * cs),
        vec2(cs, cs),
    )
}

/// Draws the board in `rect`. Returns the clicked cell, if any.
pub fn draw(ui: &mut Ui, p: &Palette, rect: Rect, game: &Game, opts: &BoardOptions) -> Option<usize> {
    let sense = if opts.interactive { Sense::click() } else { Sense::hover() };
    let resp = ui.interact(rect, egui::Id::new(opts.id), sense);
    let cs = rect.width() / 9.0;
    let pos_to_cell = |pos: egui::Pos2| -> Option<usize> {
        if !rect.contains(pos) {
            return None;
        }
        let c = (((pos.x - rect.left()) / cs) as usize).min(8);
        let r = (((pos.y - rect.top()) / cs) as usize).min(8);
        Some(r * 9 + c)
    };
    let hovered = if opts.interactive {
        resp.hover_pos().and_then(pos_to_cell)
    } else {
        None
    };
    if hovered.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let painter = ui.painter_at(rect.expand(40.0));
    let radius = CornerRadius::same(RADIUS);

    // Card + shadow.
    painter.add(p.card_shadow().as_shape(rect, radius));
    painter.rect_filled(rect, radius, p.surface);

    let value_at = |i: usize| -> u8 {
        if opts.show_solution { game.solution[i] } else { game.display_value(i) }
    };
    let sel = opts.selected;
    let sel_val = sel.map(value_at).filter(|&v| v != 0 && !opts.hide_digits);

    // ---- Cell backgrounds
    for i in 0..81 {
        let r = cell_rect(rect, i);
        let v = value_at(i);
        let mut bg = p.surface;
        if let Some(s) = sel {
            if row_of(s) == row_of(i) || col_of(s) == col_of(i) || box_of(s) == box_of(i) {
                bg = p.cell_peer;
            }
        }
        if Some(i) == hovered && Some(i) != sel {
            bg = lerp_color(bg, p.cell_hover, 0.85);
            if bg == p.surface {
                bg = p.cell_hover;
            }
        }
        if let Some(sv) = sel_val {
            if v == sv && Some(i) != sel {
                bg = p.cell_same;
            }
        }
        if opts.highlight_mistakes && !opts.hide_digits && !opts.show_solution && game.is_wrong(i) && game.is_playing() {
            bg = p.error_soft;
        }
        if Some(i) == sel {
            bg = p.cell_selected;
        }
        // Flash overlay.
        let f = opts.flashes.iter().map(|f| f.intensity(i)).fold(0.0, f32::max);
        if f > 0.0 {
            bg = lerp_color(bg, with_alpha(p.accent, 0.55), f * 0.55);
        }
        if bg != p.surface {
            let cr = CornerRadius {
                nw: if i == 0 { RADIUS } else { 0 },
                ne: if i == 8 { RADIUS } else { 0 },
                sw: if i == 72 { RADIUS } else { 0 },
                se: if i == 80 { RADIUS } else { 0 },
            };
            painter.rect_filled(r, cr, bg);
        }
    }

    // ---- Grid lines
    for k in 1..9 {
        let x = rect.left() + k as f32 * cs;
        let y = rect.top() + k as f32 * cs;
        let (w, c) = if k % 3 == 0 { (2.0, p.line_thick) } else { (1.0, p.line_thin) };
        painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(w, c));
        painter.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(w, c));
    }
    painter.rect_stroke(rect, radius, Stroke::new(2.0, p.line_thick), StrokeKind::Inside);

    // ---- Animated selection ring
    if let Some(s) = sel {
        let target = cell_rect(rect, s);
        let ctx = ui.ctx();
        let x = ctx.animate_value_with_time(egui::Id::new(opts.id).with("selx"), target.left(), 0.09);
        let y = ctx.animate_value_with_time(egui::Id::new(opts.id).with("sely"), target.top(), 0.09);
        let ring = Rect::from_min_size(pos2(x, y), target.size()).shrink(2.0);
        painter.rect_stroke(ring, CornerRadius::same(6), Stroke::new(2.0, p.accent), StrokeKind::Inside);
    }

    if opts.hide_digits {
        return None;
    }

    // ---- Digits & notes
    let digit_font = semibold(cs * 0.54);
    let note_font = regular(cs * 0.21);
    for i in 0..81 {
        let r = cell_rect(rect, i);
        let v = value_at(i);
        if v != 0 {
            let color = if game.is_given(i) {
                p.given
            } else if opts.show_solution {
                if game.cells[i] == game.solution[i] { p.user } else { p.revealed }
            } else if game.was_revealed(i) {
                p.revealed
            } else if game.hinted[i] {
                p.hinted
            } else if opts.highlight_mistakes && game.is_wrong(i) {
                p.error
            } else {
                p.user
            };
            let mut center = r.center();
            if let Some((si, t0)) = opts.shake {
                let t = t0.elapsed().as_secs_f32();
                if si == i && t < 0.4 {
                    center.x += (t * 55.0).sin() * 5.0 * (1.0 - t / 0.4);
                }
            }
            // Pop-in when a flash passes over a cell.
            let f = opts.flashes.iter().map(|f| f.intensity(i)).fold(0.0, f32::max);
            let font = if f > 0.0 { semibold(cs * 0.54 * (1.0 + 0.12 * f)) } else { digit_font.clone() };
            painter.text(center + vec2(0.0, 1.0), Align2::CENTER_CENTER, v.to_string(), font, color);

            // Conflict marker: small dot in the corner of cells clashing with a wrong entry.
            if opts.highlight_mistakes && game.is_playing() && !game.is_wrong(i) && !opts.show_solution {
                let clashes = crate::sudoku::peers(i).any(|j| game.is_wrong(j) && game.cells[j] == v);
                if clashes {
                    painter.circle_filled(pos2(r.right() - cs * 0.14, r.top() + cs * 0.14), cs * 0.05, p.error);
                }
            }
        } else if game.notes[i] != 0 && !opts.show_solution {
            let ns = cs / 3.0;
            for d in 1..=9u8 {
                if game.notes[i] & (1 << (d - 1)) == 0 {
                    continue;
                }
                let k = (d - 1) as usize;
                let c = pos2(
                    r.left() + (k % 3) as f32 * ns + ns / 2.0,
                    r.top() + (k / 3) as f32 * ns + ns / 2.0,
                );
                let highlighted = sel_val == Some(d);
                if highlighted {
                    painter.circle_filled(c, ns * 0.42, p.accent_soft);
                }
                painter.text(
                    c,
                    Align2::CENTER_CENTER,
                    d.to_string(),
                    if highlighted { semibold(cs * 0.21) } else { note_font.clone() },
                    if highlighted { p.accent } else { p.text_muted },
                );
            }
        }
    }

    if opts.interactive && resp.clicked() {
        return resp.interact_pointer_pos().and_then(pos_to_cell);
    }
    None
}
