//! Liquid-glass painting helpers for egui — light frosted glass with inner highlights,
//! sheen bands and soft shadows (Apple "liquid glass" inspired).

use egui::epaint::{CornerRadius, RectShape, Shape, Stroke, Vec2};
use egui::{Color32, Painter, Pos2, Rect, Rounding};

pub fn c_ink() -> Color32 {
    Color32::from_rgb(0x23, 0x26, 0x2E)
}
pub fn c_sub() -> Color32 {
    Color32::from_black_alpha(130)
}
pub fn c_faint() -> Color32 {
    Color32::from_black_alpha(80)
}
pub fn c_charcoal() -> Color32 {
    Color32::from_rgb(0x3E, 0x3E, 0x3E)
}
pub fn c_accent() -> Color32 {
    Color32::from_rgb(0x0A, 0x84, 0xFF)
}
pub fn c_success() -> Color32 {
    Color32::from_rgb(0x10, 0xB9, 0x81)
}
pub fn c_warn() -> Color32 {
    Color32::from_rgb(0xF5, 0x9E, 0x0B)
}
pub fn c_danger() -> Color32 {
    Color32::from_rgb(0xEF, 0x44, 0x44)
}

pub fn rad(v: f32) -> CornerRadius {
    CornerRadius::same(v.clamp(0.0, 255.0) as u8)
}

/// Paint a frosted glass panel into `rect`.
pub fn glass_panel(p: Painter, rect: Rect, radius: f32, strong: bool) {
    let r = rad(radius);
    let base_alpha = if strong { 200 } else { 135 };
    // 1. soft drop shadow (layered)
    for (i, alpha) in [10u8, 14, 18, 22].iter().enumerate() {
        let grow = (4.0 - i as f32) * 3.0;
        let sr = Rect::from_center_size(rect.center(), rect.size() + Vec2::splat(grow * 2.0));
        p.rect_filled(sr, rad(radius + grow * 0.5), Color32::from_black_alpha(*alpha));
    }
    // 2. base fill
    p.rect_filled(rect, r, Color32::from_white_alpha(base_alpha));
    // 3. sheen — top band lighter, fading down (4 bands)
    let h = rect.height();
    let bands = [0.40f32, 0.28, 0.18, 0.10];
    let mut top = rect.top();
    for (i, a) in bands.iter().enumerate() {
        let bh = h * 0.45 / bands.len() as f32;
        let band = Rect::from_min_size(
            Pos2::new(rect.left() + 1.0, top),
            Vec2::new(rect.width() - 2.0, bh + 0.5),
        );
        p.rect_filled(band, rad(radius * (1.0 - i as f32 * 0.18)), Color32::from_white_alpha((a * 255.0) as u8));
        top += bh;
    }
    // 4. inner top highlight stroke (light from top)
    p.rect_stroke(rect.shrink(0.5), r, Stroke::new(1.0, Color32::from_white_alpha(190)), egui::StrokeKind::Inside);
    // 5. subtle bottom inner shade
    let bottom = Rect::from_min_size(
        Pos2::new(rect.left() + 1.0, rect.bottom() - 2.0),
        Vec2::new(rect.width() - 2.0, 2.0),
    );
    p.rect_filled(bottom, rad(2.0), Color32::from_black_alpha(18));
}

/// Small circular glass button (like the reference `.box`): inset highlights + ring.
pub fn glass_circle(p: Painter, center: Pos2, radius: f32, hover: bool, tint: Option<Color32>) {
    let rect = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
    let r = rad(radius);
    // shadow
    p.rect_filled(rect.translate(Vec2::new(0.0, 3.0)), r, Color32::from_black_alpha(if hover { 36 } else { 24 }));
    // base
    let base = tint.unwrap_or(Color32::from_white_alpha(if hover { 235 } else { 200 }));
    p.rect_filled(rect, r, base);
    // inner highlight top / shade bottom (double inset like the reference)
    let hl = Stroke::new(1.2, Color32::from_white_alpha(220));
    let top_arc = Rect::from_min_size(Pos2::new(rect.left() + 2.0, rect.top() + 1.0), Vec2::new(rect.width() - 4.0, rect.height() * 0.5));
    p.rect_filled(top_arc, rad(radius), Color32::from_white_alpha(if hover { 110 } else { 70 }));
    let _ = hl;
    // ring
    p.rect_stroke(rect.shrink(0.5), r, Stroke::new(1.0, Color32::from_white_alpha(200)), egui::StrokeKind::Inside);
    p.rect_stroke(rect, r, Stroke::new(1.0, Color32::from_black_alpha(if hover { 70 } else { 40 })), egui::StrokeKind::Outside);
}

/// Big glossy pill button (like the reference `.start-btn`): charcoal/green glass with sheen + icon circle.
pub fn gloss_pill(p: Painter, rect: Rect, base: Color32, glow: Option<Color32>) {
    let r = rad(rect.height() / 2.0);
    // glow / shadow
    let shadow_col = glow.unwrap_or(Color32::from_black_alpha(60));
    for i in 0..3 {
        let grow = (3.0 - i as f32) * 2.0;
        let sr = Rect::from_center_size(rect.center(), rect.size() + Vec2::splat(grow * 2.0));
        let a = (60 - i * 15) as u8;
        p.rect_filled(sr, rad(sr.height() / 2.0), shadow_col.gamma_multiply(0.35));
        let _ = a;
    }
    // base
    p.rect_filled(rect, r, base);
    // sheen top
    let sheen = Rect::from_min_size(Pos2::new(rect.left() + 3.0, rect.top() + 2.0), Vec2::new(rect.width() - 6.0, rect.height() * 0.46));
    p.rect_filled(sheen, rad(sheen.height() / 2.0), Color32::from_white_alpha(60));
    // rim light
    p.rect_stroke(rect, r, Stroke::new(1.0, Color32::from_white_alpha(120)), egui::StrokeKind::Inside);
}

/// Power symbol: circle arc with gap at top + vertical stem.
pub fn power_icon(p: Painter, center: Pos2, radius: f32, color: Color32, stroke_w: f32, arc_fraction: f32) {
    let stroke = Stroke::new(stroke_w, color);
    // stem
    let stem_top = Pos2::new(center.x, center.y - radius * 1.05);
    let stem_bot = Pos2::new(center.x, center.y - radius * 0.15);
    p.line_segment([stem_top, stem_bot], stroke);
    // arc: gap at top (start at -54deg going around)
    let total = std::f32::consts::TAU * arc_fraction.clamp(0.2, 0.98);
    let start = -std::f32::consts::FRAC_PI_2 - (std::f32::consts::TAU - total) * 0.5;
    let mut pts = Vec::with_capacity(48);
    for i in 0..=48 {
        let a = start + total * (i as f32 / 48.0);
        pts.push(Pos2::new(center.x + radius * a.cos(), center.y + radius * a.sin()));
    }
    p.add(Shape::line(pts, stroke));
}

/// Country badge (2-letter code on tinted chip).
pub fn cc_badge(p: Painter, rect: Rect, cc: &str) {
    p.rect_filled(rect, rad(9.0), Color32::from_white_alpha(120));
    p.rect_stroke(rect, rad(9.0), Stroke::new(1.0, Color32::from_black_alpha(40)), egui::StrokeKind::Inside);
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        if cc.is_empty() { "??".to_string() } else { cc.to_string() },
        egui::FontId::proportional(11.5),
        c_ink(),
    );
}

/// Vertical gradient background for the whole window (fallback when no acrylic).
pub fn paint_bg(p: Painter, rect: Rect) {
    let top = Color32::from_rgb(0xF7, 0xF7, 0xFA);
    let bottom = Color32::from_rgb(0xE2, 0xE2, 0xE8);
    let steps = 64;
    let h = rect.height() / steps as f32;
    for i in 0..steps {
        let t = i as f32 / (steps - 1) as f32;
        let col = Color32::from_rgba_unmultiplied(
            (top.r() as f32 + (bottom.r() as f32 - top.r() as f32) * t) as u8,
            (top.g() as f32 + (bottom.g() as f32 - top.g() as f32) * t) as u8,
            (top.b() as f32 + (bottom.b() as f32 - top.b() as f32) * t) as u8,
            255,
        );
        let band = Rect::from_min_size(Pos2::new(rect.left(), rect.top() + h * i as f32), Vec2::new(rect.width(), h + 1.0));
        p.rect_filled(band, rad(0.0), col);
    }
    // pastel orbs (very subtle)
    p.circle_filled(Pos2::new(rect.left() + rect.width() * 0.15, rect.top() + rect.height() * 0.12), 160.0, Color32::from_rgba_unmultiplied(0xC7, 0xD2, 0xFE, 60));
    p.circle_filled(Pos2::new(rect.right() - rect.width() * 0.10, rect.top() + rect.height() * 0.22), 130.0, Color32::from_rgba_unmultiplied(0xA5, 0xF3, 0xFC, 50));
    p.circle_filled(Pos2::new(rect.left() + rect.width() * 0.55, rect.bottom() - 60.0), 150.0, Color32::from_rgba_unmultiplied(0xFB, 0xCF, 0xE8, 45));
}

pub fn rect_shape(rect: Rect, r: CornerRadius, fill: Color32) -> Shape {
    Shape::Rect(RectShape::filled(rect, r, fill))
}
