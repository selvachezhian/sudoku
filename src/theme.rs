//! Visual design system: palettes for light & dark mode, fonts and egui style.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke,
    TextStyle, Visuals, epaint::Shadow,
};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub surface: Color32,
    pub surface_alt: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub given: Color32,
    pub user: Color32,
    pub hinted: Color32,
    pub revealed: Color32,
    pub error: Color32,
    pub error_soft: Color32,
    pub success: Color32,
    pub success_soft: Color32,
    pub warning: Color32,
    pub cell_selected: Color32,
    pub cell_peer: Color32,
    pub cell_same: Color32,
    pub cell_hover: Color32,
    pub line_thin: Color32,
    pub line_thick: Color32,
    pub shadow: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

impl Palette {
    pub fn light() -> Self {
        Palette {
            dark: false,
            bg: hex(0xF3F5FA),
            surface: hex(0xFFFFFF),
            surface_alt: hex(0xF7F8FC),
            border: hex(0xE3E7F0),
            text: hex(0x161A26),
            text_muted: hex(0x5B6478),
            text_faint: hex(0x98A1B3),
            accent: hex(0x4F5BD5),
            accent_hover: hex(0x4350C4),
            accent_soft: hex(0xE8EBFF),
            on_accent: hex(0xFFFFFF),
            given: hex(0x1B2033),
            user: hex(0x3D4BD1),
            hinted: hex(0x0E9F6E),
            revealed: hex(0xB4589F),
            error: hex(0xE5484D),
            error_soft: hex(0xFDE8E8),
            success: hex(0x12A150),
            success_soft: hex(0xE3F7EC),
            warning: hex(0xE8A13A),
            cell_selected: hex(0xC9D1FF),
            cell_peer: hex(0xEDF0FA),
            cell_same: hex(0xDCE1FF),
            cell_hover: hex(0xF1F3FB),
            line_thin: hex(0xDDE2EC),
            line_thick: hex(0x3A4058),
            shadow: Color32::from_black_alpha(28),
        }
    }

    pub fn dark() -> Self {
        Palette {
            dark: true,
            bg: hex(0x0E1016),
            surface: hex(0x171A23),
            surface_alt: hex(0x1D212C),
            border: hex(0x2A2F3D),
            text: hex(0xE9ECF4),
            text_muted: hex(0x9AA3B8),
            text_faint: hex(0x5F687C),
            accent: hex(0x7C87FF),
            accent_hover: hex(0x8F99FF),
            accent_soft: hex(0x262B4A),
            on_accent: hex(0x0E1016),
            given: hex(0xE9ECF4),
            user: hex(0x8D98FF),
            hinted: hex(0x34D399),
            revealed: hex(0xE58AD0),
            error: hex(0xFF6B6F),
            error_soft: hex(0x3A1E24),
            success: hex(0x34D399),
            success_soft: hex(0x16302A),
            warning: hex(0xF5B54F),
            cell_selected: hex(0x343C72),
            cell_peer: hex(0x1F2433),
            cell_same: hex(0x2A3160),
            cell_hover: hex(0x222736),
            line_thin: hex(0x2B3040),
            line_thick: hex(0x8890A8),
            shadow: Color32::from_black_alpha(90),
        }
    }

    pub fn get(dark: bool) -> Self {
        if dark { Self::dark() } else { Self::light() }
    }

    pub fn card_shadow(&self) -> Shadow {
        Shadow {
            offset: [0, 6],
            blur: 24,
            spread: 0,
            color: self.shadow,
        }
    }
}

pub const SEMIBOLD: &str = "semibold";
pub const BOLD: &str = "bold";

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}
pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "inter".into(),
        Arc::new(FontData::from_static(include_bytes!("../assets/fonts/Inter-400.ttf"))),
    );
    fonts.font_data.insert(
        "inter-semibold".into(),
        Arc::new(FontData::from_static(include_bytes!("../assets/fonts/Inter-500.ttf"))),
    );
    fonts.font_data.insert(
        "inter-bold".into(),
        Arc::new(FontData::from_static(include_bytes!("../assets/fonts/Inter-700.ttf"))),
    );

    // Keep egui's default fonts as fallbacks (they provide the emoji/icon glyphs).
    let fallbacks = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "inter".into());

    let mut semi = vec!["inter-semibold".to_string()];
    semi.extend(fallbacks.iter().cloned());
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), semi);

    let mut bold = vec!["inter-bold".to_string()];
    bold.extend(fallbacks.iter().cloned());
    fonts.families.insert(FontFamily::Name(BOLD.into()), bold);

    ctx.set_fonts(fonts);
}

pub fn apply_style(ctx: &egui::Context, p: &Palette) {
    let mut visuals = if p.dark { Visuals::dark() } else { Visuals::light() };
    visuals.panel_fill = p.bg;
    visuals.window_fill = p.surface;
    visuals.window_stroke = Stroke::new(1.0, p.border);
    visuals.window_corner_radius = CornerRadius::same(18);
    visuals.window_shadow = Shadow {
        offset: [0, 12],
        blur: 40,
        spread: 0,
        color: p.shadow,
    };
    visuals.popup_shadow = p.card_shadow();
    visuals.extreme_bg_color = p.surface_alt;
    visuals.faint_bg_color = p.surface_alt;
    visuals.selection.bg_fill = p.accent;
    visuals.selection.stroke = Stroke::new(1.0, p.on_accent);
    visuals.hyperlink_color = p.accent;
    visuals.override_text_color = Some(p.text);

    let radius = CornerRadius::same(10);
    for w in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        w.corner_radius = radius;
    }
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    visuals.widgets.inactive.bg_fill = p.surface_alt;
    visuals.widgets.inactive.weak_bg_fill = p.surface_alt;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    visuals.widgets.hovered.bg_fill = p.cell_hover;
    visuals.widgets.hovered.weak_bg_fill = p.cell_hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, p.text);
    visuals.widgets.active.bg_fill = p.accent_soft;
    visuals.widgets.active.weak_bg_fill = p.accent_soft;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5, p.text);

    ctx.set_visuals(visuals);

    ctx.style_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, semibold(22.0)),
            (TextStyle::Body, regular(14.5)),
            (TextStyle::Button, regular(14.5)),
            (TextStyle::Small, regular(12.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(14.0, 8.0);
        style.spacing.interact_size.y = 34.0;
        style.spacing.scroll.bar_width = 8.0;
        style.animation_time = 0.14;
    });
}
