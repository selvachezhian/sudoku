//! Hand-painted widgets for a polished, consistent look.

use crate::theme::{Palette, regular, semibold};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontId, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, Vec2, pos2, vec2,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Primary,
    Secondary,
    Ghost,
    Danger,
    DangerGhost,
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
        l(a.a(), b.a()),
    )
}

pub fn with_alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

fn interact(ui: &mut Ui, size: Vec2, enabled: bool) -> (Rect, Response, f32, f32) {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let resp = if enabled {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    };
    let hover = ui
        .ctx()
        .animate_bool_with_time(resp.id.with("hover"), enabled && resp.hovered(), 0.12);
    let press = ui
        .ctx()
        .animate_bool_with_time(resp.id.with("press"), enabled && resp.is_pointer_button_down_on(), 0.06);
    (rect, resp, hover, press)
}

/// A rounded button with consistent styling. `width` of 0 means "fit content".
pub fn button(ui: &mut Ui, p: &Palette, text: &str, kind: Kind, width: f32, height: f32, enabled: bool) -> Response {
    let font = semibold(14.5);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE);
    let w = width.max(galley.size().x + 32.0);
    let (rect, resp, hover, press) = interact(ui, vec2(w, height), enabled);
    if !ui.is_rect_visible(rect) {
        return resp;
    }

    let (fill, fill_hover, stroke, text_color) = match kind {
        Kind::Primary => (p.accent, p.accent_hover, Color32::TRANSPARENT, p.on_accent),
        Kind::Secondary => (p.surface, p.cell_hover, p.border, p.text),
        Kind::Ghost => (Color32::TRANSPARENT, p.cell_hover, Color32::TRANSPARENT, p.text_muted),
        Kind::Danger => (p.error, lerp_color(p.error, Color32::BLACK, 0.12), Color32::TRANSPARENT, Color32::WHITE),
        Kind::DangerGhost => (Color32::TRANSPARENT, p.error_soft, with_alpha(p.error, 0.35), p.error),
    };
    let mut fill = lerp_color(fill, fill_hover, hover);
    let mut text_color = text_color;
    if !enabled {
        fill = with_alpha(fill, if fill.a() == 0 { 0.0 } else { 0.45 });
        text_color = with_alpha(text_color, 0.4);
    }
    let r = rect.shrink(press * 1.0);
    let painter = ui.painter();
    painter.rect_filled(r, CornerRadius::same(11), fill);
    if stroke.a() > 0 {
        painter.rect_stroke(r, CornerRadius::same(11), Stroke::new(1.0, stroke), StrokeKind::Inside);
    }
    painter.text(r.center(), Align2::CENTER_CENTER, text, font, text_color);
    resp
}

/// Square icon button (used in the top bar).
pub fn icon_button(ui: &mut Ui, p: &Palette, icon: &str, tooltip: &str) -> Response {
    let (rect, resp, hover, press) = interact(ui, vec2(38.0, 38.0), true);
    let painter = ui.painter();
    painter.rect_filled(
        rect.shrink(press),
        CornerRadius::same(10),
        lerp_color(Color32::TRANSPARENT, p.cell_hover, hover),
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon,
        regular(18.0),
        lerp_color(p.text_muted, p.text, hover),
    );
    resp.on_hover_text(tooltip)
}

/// Tool tile with an icon and a label underneath (Undo, Erase, Notes, Hint).
pub fn tool_button(
    ui: &mut Ui,
    p: &Palette,
    icon: &str,
    label: &str,
    size: Vec2,
    active: bool,
    enabled: bool,
    badge: Option<&str>,
) -> Response {
    let (rect, resp, hover, press) = interact(ui, size, enabled);
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let act = ui.ctx().animate_bool_with_time(resp.id.with("active"), active, 0.15);
    let base = lerp_color(p.surface_alt, p.cell_hover, hover);
    let fill = lerp_color(base, p.accent_soft, act);
    let fg = if enabled {
        lerp_color(lerp_color(p.text_muted, p.text, hover), p.accent, act)
    } else {
        with_alpha(p.text_faint, 0.6)
    };
    let r = rect.shrink(press * 1.5);
    let painter = ui.painter();
    painter.rect_filled(r, CornerRadius::same(14), fill);
    painter.rect_stroke(
        r,
        CornerRadius::same(14),
        Stroke::new(1.0, lerp_color(p.border, p.accent, act * 0.6)),
        StrokeKind::Inside,
    );
    painter.text(
        pos2(r.center().x, r.center().y - 9.0),
        Align2::CENTER_CENTER,
        icon,
        regular(20.0),
        fg,
    );
    painter.text(
        pos2(r.center().x, r.bottom() - 15.0),
        Align2::CENTER_CENTER,
        label,
        semibold(12.0),
        fg,
    );
    if let Some(b) = badge {
        let font = semibold(10.0);
        let g = painter.layout_no_wrap(b.to_owned(), font.clone(), Color32::WHITE);
        let w = (g.size().x + 10.0).max(18.0);
        let br = Rect::from_center_size(pos2(r.right() - w / 2.0 - 4.0, r.top() + 11.0), vec2(w, 16.0));
        let bg = if active { p.accent } else { p.text_faint };
        painter.rect_filled(br, CornerRadius::same(8), bg);
        painter.text(br.center(), Align2::CENTER_CENTER, b, font, p.on_accent);
    }
    resp
}

/// Number pad key showing the digit and how many are left to place.
pub fn num_key(ui: &mut Ui, p: &Palette, digit: u8, remaining: usize, notes_mode: bool, size: Vec2, enabled: bool) -> Response {
    let done = remaining == 0;
    let (rect, resp, hover, press) = interact(ui, size, enabled && !done);
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let r = rect.shrink(press * 1.5);
    let painter = ui.painter();
    let fill = lerp_color(p.surface_alt, p.accent_soft, hover);
    painter.rect_filled(r, CornerRadius::same(14), fill);
    painter.rect_stroke(
        r,
        CornerRadius::same(14),
        Stroke::new(1.0, lerp_color(p.border, p.accent, hover * 0.7)),
        StrokeKind::Inside,
    );
    let color = if done || !enabled {
        p.text_faint
    } else if notes_mode {
        p.text_muted
    } else {
        lerp_color(p.user, p.accent_hover, hover)
    };
    let font = if notes_mode { regular(size.y * 0.36) } else { semibold(size.y * 0.44) };
    painter.text(
        pos2(r.center().x, r.center().y - 5.0),
        Align2::CENTER_CENTER,
        digit.to_string(),
        font,
        color,
    );
    let sub = if done { "✔".to_string() } else { remaining.to_string() };
    painter.text(
        pos2(r.center().x, r.bottom() - 11.0),
        Align2::CENTER_CENTER,
        sub,
        regular(10.5),
        if done { p.success } else { p.text_faint },
    );
    resp
}

/// Segmented control with an animated sliding indicator. Returns true on change.
pub fn segmented(ui: &mut Ui, p: &Palette, id: &str, options: &[&str], selected: &mut usize, width: f32, height: f32) -> bool {
    let n = options.len().max(1);
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let seg_w = (width - 8.0) / n as f32;
    let painter = ui.painter().clone();
    painter.rect_filled(rect, CornerRadius::same(12), p.surface_alt);
    painter.rect_stroke(rect, CornerRadius::same(12), Stroke::new(1.0, p.border), StrokeKind::Inside);

    let target_x = rect.left() + 4.0 + seg_w * *selected as f32;
    let x = ui
        .ctx()
        .animate_value_with_time(egui::Id::new(id).with("seg"), target_x, 0.18);
    let pill = Rect::from_min_size(pos2(x, rect.top() + 4.0), vec2(seg_w, height - 8.0));
    painter.add(p.card_shadow_small().as_shape(pill, CornerRadius::same(9)));
    painter.rect_filled(pill, CornerRadius::same(9), if p.dark { p.cell_selected } else { p.surface });

    let mut changed = false;
    for (k, label) in options.iter().enumerate() {
        let r = Rect::from_min_size(pos2(rect.left() + 4.0 + seg_w * k as f32, rect.top() + 4.0), vec2(seg_w, height - 8.0));
        let resp = ui
            .interact(r, egui::Id::new(id).with(k), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        if resp.clicked() && *selected != k {
            *selected = k;
            changed = true;
        }
        let is_sel = *selected == k;
        let color = if is_sel {
            p.text
        } else if resp.hovered() {
            p.text
        } else {
            p.text_muted
        };
        let font = if is_sel { semibold(13.5) } else { regular(13.5) };
        painter.text(r.center(), Align2::CENTER_CENTER, *label, font, color);
    }
    changed
}

/// Small coloured pill (difficulty / status badges).
pub fn badge(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let font = semibold(11.5);
    let g = ui.painter().layout_no_wrap(text.to_owned(), font.clone(), color);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 18.0, 22.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(11), with_alpha(color, 0.14));
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, font, color);
    resp
}

/// A small label/value stat block.
pub fn stat(ui: &mut Ui, p: &Palette, label: &str, value: &str, value_color: Color32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 46.0), Sense::hover());
    let painter = ui.painter();
    painter.text(
        pos2(rect.left(), rect.top() + 2.0),
        Align2::LEFT_TOP,
        label.to_uppercase(),
        semibold(10.5),
        p.text_faint,
    );
    painter.text(
        pos2(rect.left(), rect.bottom() - 2.0),
        Align2::LEFT_BOTTOM,
        value,
        semibold(19.0),
        value_color,
    );
}

pub fn progress_bar(ui: &mut Ui, p: &Palette, frac: f32, width: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 6.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(3), p.border);
    let mut fill = rect;
    fill.set_width((rect.width() * frac.clamp(0.0, 1.0)).max(if frac > 0.0 { 6.0 } else { 0.0 }));
    if frac > 0.0 {
        painter.rect_filled(fill, CornerRadius::same(3), color);
    }
}

pub fn section_title(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(egui::RichText::new(text.to_uppercase()).font(semibold(11.0)).color(p.text_faint));
}

pub fn text(t: &str, font: FontId, color: Color32) -> egui::RichText {
    egui::RichText::new(t).font(font).color(color)
}

impl Palette {
    pub fn card_shadow_small(&self) -> egui::epaint::Shadow {
        egui::epaint::Shadow {
            offset: [0, 1],
            blur: 4,
            spread: 0,
            color: if self.dark {
                Color32::from_black_alpha(60)
            } else {
                Color32::from_black_alpha(22)
            },
        }
    }

    pub fn difficulty_color(&self, d: crate::sudoku::Difficulty) -> Color32 {
        use crate::sudoku::Difficulty::*;
        match d {
            Easy => self.success,
            Medium => self.accent,
            Hard => self.warning,
            Expert => self.error,
        }
    }

    pub fn status_color(&self, s: crate::game::Status) -> Color32 {
        use crate::game::Status::*;
        match s {
            InProgress => self.accent,
            Completed => self.success,
            Revealed => self.revealed,
        }
    }
}
