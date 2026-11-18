use egui::{Color32, Context, Frame, Margin, Response, RichText, Ui};

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
    if dark {
        DARK
    } else {
        LIGHT
    }
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
    // Slider rails and inactive controls use this fill; card-on-card would be
    // invisible, so step it up one level.
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
        (
            TextStyle::Heading,
            FontId::new(26.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(13.5, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(11.5, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(12.5, FontFamily::Monospace),
        ),
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

/// The wordmark: a play triangle inside a soft square, plus three field rows.
pub fn logo(ui: &mut Ui, p: &Palette, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, size * 0.26, p.accent);
    let inner = rect.shrink(size * 0.24);
    let center = inner.center();
    let triangle = vec![
        egui::pos2(center.x - inner.width() * 0.18, inner.top()),
        egui::pos2(center.x - inner.width() * 0.18, inner.bottom()),
        egui::pos2(inner.right(), center.y),
    ];
    painter.add(egui::Shape::convex_polygon(
        triangle,
        p.accent_ink,
        egui::Stroke::NONE,
    ));
    let bar = size * 0.06;
    let y = rect.bottom() - size * 0.12;
    painter.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(rect.left() + size * 0.2, y),
            egui::vec2(size * 0.22, bar),
        ),
        1.0,
        p.accent_ink,
    );
    painter.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(rect.left() + size * 0.48, y),
            egui::vec2(size * 0.08, bar),
        ),
        1.0,
        p.accent_ink,
    );
    painter.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(rect.left() + size * 0.62, y),
            egui::vec2(size * 0.22, bar),
        ),
        1.0,
        p.accent_ink,
    );
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
