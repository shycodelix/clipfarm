use egui::{Color32, Context, FontFamily, FontId, Frame, Margin, Response, RichText, Ui};

/// Warm ink, one loud coral, a mint for good news. Deliberately not the default
/// "AI dashboard" purple-and-teal.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub card: Color32,
    pub card_alt: Color32,
    pub ink: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub line: Color32,
    pub accent: Color32,
    pub accent_ink: Color32,
    pub mint: Color32,
    pub amber: Color32,
}

pub const DARK: Palette = Palette {
    dark: true,
    bg: Color32::from_rgb(14, 15, 18),
    panel: Color32::from_rgb(20, 22, 27),
    card: Color32::from_rgb(27, 30, 37),
    card_alt: Color32::from_rgb(34, 37, 45),
    ink: Color32::from_rgb(242, 239, 233),
    muted: Color32::from_rgb(154, 160, 172),
    faint: Color32::from_rgb(96, 101, 112),
    line: Color32::from_rgb(44, 48, 57),
    accent: Color32::from_rgb(255, 107, 74),
    accent_ink: Color32::from_rgb(26, 12, 8),
    mint: Color32::from_rgb(116, 224, 168),
    amber: Color32::from_rgb(245, 194, 107),
};

pub const LIGHT: Palette = Palette {
    dark: false,
    bg: Color32::from_rgb(247, 244, 237),
    panel: Color32::from_rgb(240, 236, 227),
    card: Color32::from_rgb(255, 253, 248),
    card_alt: Color32::from_rgb(236, 231, 220),
    ink: Color32::from_rgb(26, 24, 22),
    muted: Color32::from_rgb(101, 96, 89),
    faint: Color32::from_rgb(148, 141, 132),
    line: Color32::from_rgb(216, 209, 197),
    accent: Color32::from_rgb(224, 76, 48),
    accent_ink: Color32::from_rgb(255, 250, 244),
    mint: Color32::from_rgb(28, 122, 80),
    amber: Color32::from_rgb(150, 96, 18),
};

pub fn palette(dark: bool) -> Palette {
    if dark { DARK } else { LIGHT }
}

/// Apply typography and widget styling. Called every frame; cheap and keeps
/// both themes consistent even after egui style changes elsewhere.
pub fn apply(ctx: &Context, p: &Palette) {
    let mut visuals = if p.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = p.bg;
    visuals.window_fill = p.panel;
    visuals.extreme_bg_color = if p.dark {
        Color32::from_rgb(11, 12, 15)
    } else {
        Color32::from_rgb(238, 233, 223)
    };
    visuals.override_text_color = Some(p.ink);
    visuals.faint_bg_color = p.card_alt;
    visuals.widgets.noninteractive.bg_fill = p.panel;
    visuals.widgets.inactive.bg_fill = p.card_alt;
    visuals.widgets.inactive.weak_bg_fill = p.card_alt;
    visuals.widgets.hovered.bg_fill = p.card_alt;
    visuals.widgets.hovered.weak_bg_fill = p.card_alt;
    visuals.widgets.active.bg_fill = p.accent;
    visuals.selection.bg_fill = if p.dark {
        Color32::from_rgb(96, 45, 33)
    } else {
        Color32::from_rgb(250, 214, 203)
    };
    visuals.hyperlink_color = p.accent;
    visuals.window_stroke = egui::Stroke::new(1.0_f32, p.line);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    use egui::{FontFamily, FontId, TextStyle};
    style.text_styles = [
        (TextStyle::Heading, FontId::new(26.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(13.5, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.slider_width = 170.0;
    ctx.set_style(style);
}

pub fn card_frame(p: &Palette) -> Frame {
    Frame::default()
        .fill(p.card)
        .stroke(egui::Stroke::new(1.0_f32, p.line))
        .inner_margin(Margin::same(18))
}

pub fn flat_frame(p: &Palette) -> Frame {
    Frame::default()
        .fill(p.panel)
        .inner_margin(Margin::same(14))
}

pub fn primary(ui: &mut Ui, p: &Palette, label: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(p.accent_ink).strong())
            .fill(p.accent)
            .min_size(egui::vec2(0.0, 36.0)),
    )
}

pub fn secondary(ui: &mut Ui, p: &Palette, label: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(p.ink))
            .fill(p.card_alt)
            .min_size(egui::vec2(0.0, 32.0)),
    )
}

pub fn quiet(ui: &mut Ui, p: &Palette, label: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(p.muted).size(12.0))
            .frame(false)
            .min_size(egui::vec2(0.0, 26.0)),
    )
}

pub fn kicker(ui: &mut Ui, p: &Palette, text: &str) -> Response {
    ui.label(
        RichText::new(text.to_ascii_uppercase())
            .size(10.5)
            .strong()
            .color(p.faint),
    )
}

pub fn chip(ui: &mut Ui, fill: Color32, text_color: Color32, text: &str) -> Response {
    Frame::default()
        .fill(fill)
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(10.5).strong().color(text_color))
        })
        .inner
}

/// Lighten an sRGB color toward white by `t` (0 = unchanged, 1 = white).
fn lighten(c: Color32, t: f32) -> Color32 {
    let t = t.max(0.0).min(1.0);
    Color32::from_rgb(
        (c.r() as f32 + (255.0 - c.r() as f32) * t) as u8,
        (c.g() as f32 + (255.0 - c.g() as f32) * t) as u8,
        (c.b() as f32 + (255.0 - c.b() as f32) * t) as u8,
    )
}

/// The clipfarm mark: a coral squircle with a soft upper-left sheen, a dark
/// rounded "clip card" containing a white play triangle, and three croprow ticks
/// near the card's bottom edge (the reframe-to-9:16 signature).
///
/// Geometry is scaled from scripts/make-icon.py (1024px icon) so the in-app mark
/// reads as the same logo as the app icon:
/// - squircle: full mark rect, corner radius 224/1024.
/// - sheen: soft highlight circle centred at (0.30s, 0.22s), radius 0.21s.
/// - card: square, side 0.5859s, centred at (s/2, s/2 - 30), corner radius 78/1024.
/// - play triangle (card-local fractions): (-0.125,-0.08),(-0.125,0.08),(0.045,0).
/// - three ticks (card-local): centres (-0.1783,0),(0,0),(0.1617,0), widths
///   (0.25,0.0367,0.25), height 0.0767, at card-local y +0.425.
pub fn mark(painter: &egui::Painter, rect: egui::Rect, p: &Palette) {
    let s = rect.width().min(rect.height());
    let cx = rect.center().x;
    let cy = rect.center().y;

    // Coral squircle background.
    painter.rect_filled(
        rect,
        s * 0.21875, // 224 / 1024
        p.accent,
    );

    // Soft upper-left sheen: concentric circles that approximate the icon's radial
    // highlight (clamp(1 - dist/737.3) capped at 0.24). The largest circle has radius
    // 0.215s, which stays inside the squircle (the squircle boundary is ~0.220s away
    // from the sheen centre (0.30s, 0.22s) at the top edge).
    let sheen_cx = s * 0.30;
    let sheen_cy = s * 0.22;
    let sheen_r = s * 0.215;
    for frac in [1.0, 0.75, 0.5, 0.25, 0.0] {
        let r = sheen_r * frac as f32;
        let blend = 0.24 * (1.0 - r / (s * 0.72));
        painter.circle_filled(egui::pos2(sheen_cx, sheen_cy), r, lighten(p.accent, blend));
    }

    // Dark rounded "clip card" (square, offset up by 30/1024 of s).
    let card_size = s * 0.5859;
    let card_cx = cx;
    let card_cy = cy - s * 0.0293;
    let card_r = s * 0.0762; // 78 / 1024
    let card = egui::Rect::from_center_size(
        egui::pos2(card_cx, card_cy),
        egui::vec2(card_size, card_size),
    );
    painter.rect_filled(card, card_r, p.accent_ink);
    painter.rect_stroke(
        card,
        card_r,
        egui::Stroke::new(s * 0.00293, lighten(Color32::from_rgb(255, 255, 255), 0.25)),
        egui::StrokeKind::Outside,
    );

    // Play triangle in warm off-white.
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(card_cx - card_size * 0.125, card_cy - card_size * 0.08),
            egui::pos2(card_cx - card_size * 0.125, card_cy + card_size * 0.08),
            egui::pos2(card_cx + card_size * 0.045, card_cy),
        ],
        Color32::from_rgb(250, 244, 236),
        egui::Stroke::NONE,
    ));

    // Three croprow ticks near the card's bottom edge.
    let tick_cy = card_cy + card_size * 0.425;
    let tick_h = card_size * 0.0767; // 46 / 600
    let tick_r = tick_h * 0.5;
    for (cx_frac, w_frac) in [(-0.1783, 0.25), (0.0, 0.0367), (0.1617, 0.25)] {
        painter.rect_filled(
            egui::Rect::from_center_size(
                egui::pos2(card_cx + card_size * cx_frac, tick_cy),
                egui::vec2(card_size * w_frac, tick_h),
            ),
            tick_r,
            p.accent_ink,
        );
    }
}

/// Full lockup: mark + tracked "clipfarm" wordmark + "local clip studio" eyebrow.
/// Used in the sidebar and the farm-screen header. Returns the width consumed.
pub fn brand(ui: &mut Ui, p: &Palette, logomark_size: f32) -> f32 {
    let (mark_rect, _) = ui.allocate_exact_size(
        egui::vec2(logomark_size, logomark_size),
        egui::Sense::hover(),
    );
    mark(&ui.painter(), mark_rect, p);

    let word = "clipfarm";
    let eyebrow = "local clip studio";
    let wordsize = logomark_size * 0.26;
    let advance = wordsize * 0.66;
    let baseline = mark_rect.center().y + wordsize * 0.18;
    let text_start = mark_rect.right() + logomark_size * 0.07;

    let painter = ui.painter();
    let mut x = text_start;
    for ch in word.chars() {
        painter.text(
            egui::pos2(x, baseline),
            egui::Align2::LEFT_BOTTOM,
            ch.to_string(),
            FontId::new(wordsize, FontFamily::Proportional),
            p.ink,
        );
        x += advance;
    }

    painter.text(
        egui::pos2(text_start, baseline + wordsize * 0.55),
        egui::Align2::LEFT_TOP,
        eyebrow,
        FontId::new(wordsize * 0.42, FontFamily::Proportional),
        p.faint,
    );

    logomark_size + logomark_size * 0.07 + word.len() as f32 * advance
}

pub fn progress_bar(ui: &mut Ui, p: &Palette, fraction: f32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 8.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 4.0, p.card_alt);
    let filled = egui::Rect::from_min_size(
        rect.min,
        egui::vec2(rect.width() * fraction.clamp(0.0, 1.0), rect.height()),
    );
    ui.painter().rect_filled(filled, 4.0, p.accent);
}
