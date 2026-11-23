use eframe::egui::{self, Color32, Painter, Pos2, Rect, Stroke, Vec2};

/// Paints the custom geometric Simpul Knot ("S" interwoven creative emblem) in the header.
pub fn paint_simpul_brand_mark(painter: &Painter, rect: Rect) {
    let radius = rect.width() * 0.28;
    // Deep cobalt-indigo base badge
    painter.rect_filled(rect, radius, Color32::from_rgb(78, 123, 251));
    let inner = rect.shrink(1.5);
    painter.rect_stroke(
        inner,
        radius - 1.0,
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 55)),
        egui::StrokeKind::Inside,
    );

    let c = rect.center();
    let u = rect.width() * 0.23;
    let stroke_white = Stroke::new(2.4_f32, Color32::from_rgb(248, 250, 255));
    let stroke_cyan = Stroke::new(2.0_f32, Color32::from_rgb(165, 243, 252));

    // Interlocking Simpul ("Knot") double loop
    let top_loop = Pos2::new(c.x, c.y - u * 0.52);
    let bot_loop = Pos2::new(c.x, c.y + u * 0.52);
    painter.circle_stroke(top_loop, u * 0.72, stroke_white);
    painter.circle_stroke(bot_loop, u * 0.72, stroke_cyan);

    // Diagonal bridge across knot center
    painter.line_segment(
        [
            Pos2::new(c.x - u * 0.65, c.y - u * 0.15),
            Pos2::new(c.x + u * 0.65, c.y + u * 0.15),
        ],
        stroke_white,
    );
    painter.circle_filled(c, 2.3, Color32::WHITE);
}

/// Paints custom procedural vector artwork for each Simpul Creative application.
pub fn paint_app_icon(painter: &Painter, rect: Rect, icon_kind: &str, accent: [u8; 3]) {
    let base = Color32::from_rgb(accent[0], accent[1], accent[2]);
    let dark_tint = Color32::from_rgb(
        (accent[0] as u16 * 32 / 100) as u8,
        (accent[1] as u16 * 32 / 100) as u8,
        (accent[2] as u16 * 38 / 100) as u8,
    );
    let corner = (rect.width() * 0.24).clamp(8.0, 16.0);

    painter.rect_filled(rect, corner, dark_tint);
    // Subtle top-left glow
    let glow_rect = Rect::from_min_size(rect.min, Vec2::new(rect.width(), rect.height() * 0.52));
    painter.rect_filled(
        glow_rect,
        corner,
        Color32::from_rgba_unmultiplied(accent[0], accent[1], accent[2], 48),
    );
    painter.rect_stroke(
        rect,
        corner,
        Stroke::new(1.3_f32, Color32::from_rgba_unmultiplied(accent[0], accent[1], accent[2], 190)),
        egui::StrokeKind::Inside,
    );

    let c = rect.center();
    let s = rect.width();
    let fg = Color32::from_rgb(245, 247, 252);
    let stroke_fg = Stroke::new((s * 0.048).clamp(1.6, 2.8), fg);
    let stroke_accent = Stroke::new((s * 0.048).clamp(1.6, 2.8), base);

    match icon_kind {
        "gram" => {
            // SimpulGram: Aperture frame + lens + live broadcast spark
            let frame = rect.shrink(s * 0.23);
            painter.rect_stroke(frame, s * 0.12, stroke_fg, egui::StrokeKind::Middle);
            painter.circle_stroke(c, s * 0.13, stroke_accent);
            painter.circle_filled(
                Pos2::new(frame.right() - s * 0.08, frame.top() + s * 0.08),
                (s * 0.038).max(1.8),
                base,
            );
        }
        "sync4" => {
            // SimpulSync 4: Dual orbital sync ring + central "4" / node core
            let r = s * 0.25;
            painter.circle_stroke(c, r, stroke_accent);
            painter.line_segment(
                [Pos2::new(c.x - r, c.y), Pos2::new(c.x - r + s * 0.09, c.y - s * 0.08)],
                stroke_fg,
            );
            painter.line_segment(
                [Pos2::new(c.x + r, c.y), Pos2::new(c.x + r - s * 0.09, c.y + s * 0.08)],
                stroke_fg,
            );
            // Crisp "4" indicator in center
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "4",
                egui::FontId::proportional((s * 0.30).clamp(11.0, 20.0)),
                fg,
            );
        }
        "capr" => {
            // Capr Notebook: Spiral notebook spine + live execution cell prompt `<>`
            let book = Rect::from_center_size(c, Vec2::new(s * 0.48, s * 0.56));
            painter.rect_stroke(book, s * 0.07, stroke_fg, egui::StrokeKind::Middle);
            for i in 0..3 {
                let ry = book.top() + s * (0.13 + i as f32 * 0.15);
                painter.line_segment(
                    [
                        Pos2::new(book.left() - s * 0.05, ry),
                        Pos2::new(book.left() + s * 0.05, ry),
                    ],
                    stroke_accent,
                );
            }
            // Cell play/chevron mark inside notebook
            let cx_in = c.x + s * 0.02;
            painter.line_segment(
                [
                    Pos2::new(cx_in - s * 0.07, c.y - s * 0.09),
                    Pos2::new(cx_in + s * 0.08, c.y),
                ],
                stroke_accent,
            );
            painter.line_segment(
                [
                    Pos2::new(cx_in + s * 0.08, c.y),
                    Pos2::new(cx_in - s * 0.07, c.y + s * 0.09),
                ],
                stroke_accent,
            );
        }
        "docs" => {
            // SimpulDocs: Document sheet with structured bars
            let doc = Rect::from_center_size(c, Vec2::new(s * 0.44, s * 0.54));
            painter.rect_stroke(doc, s * 0.06, stroke_fg, egui::StrokeKind::Middle);
            for i in 0..3 {
                let y = doc.top() + s * (0.15 + i as f32 * 0.12);
                let right = if i == 2 { doc.right() - s * 0.14 } else { doc.right() - s * 0.08 };
                painter.line_segment(
                    [Pos2::new(doc.left() + s * 0.08, y), Pos2::new(right, y)],
                    if i == 0 { stroke_accent } else { stroke_fg },
                );
            }
        }
        "flow" => {
            // SimpulFlow: CLI prompt `>_` + pipeline node
            let left = c.x - s * 0.20;
            painter.line_segment(
                [Pos2::new(left, c.y - s * 0.13), Pos2::new(left + s * 0.14, c.y)],
                stroke_accent,
            );
            painter.line_segment(
                [Pos2::new(left + s * 0.14, c.y), Pos2::new(left, c.y + s * 0.13)],
                stroke_accent,
            );
            painter.line_segment(
                [
                    Pos2::new(c.x + s * 0.02, c.y + s * 0.13),
                    Pos2::new(c.x + s * 0.22, c.y + s * 0.13),
                ],
                stroke_fg,
            );
        }
        "studio" => {
            // SimpulStudio: Connected creative node graph
            let p1 = Pos2::new(c.x - s * 0.17, c.y - s * 0.14);
            let p2 = Pos2::new(c.x - s * 0.17, c.y + s * 0.14);
            let p3 = Pos2::new(c.x + s * 0.18, c.y);
            painter.line_segment([p1, p3], stroke_accent);
            painter.line_segment([p2, p3], stroke_accent);
            for p in [p1, p2] {
                painter.circle_filled(p, (s * 0.065).max(2.4), fg);
            }
            painter.circle_filled(p3, (s * 0.085).max(3.0), base);
        }
        "canvas" => {
            // SimpulCanvas: Layered design artboards
            let back = Rect::from_center_size(
                Pos2::new(c.x + s * 0.06, c.y + s * 0.06),
                Vec2::new(s * 0.38, s * 0.38),
            );
            let front = Rect::from_center_size(
                Pos2::new(c.x - s * 0.05, c.y - s * 0.05),
                Vec2::new(s * 0.38, s * 0.38),
            );
            painter.rect_stroke(back, s * 0.06, stroke_accent, egui::StrokeKind::Middle);
            painter.rect_filled(front, s * 0.06, dark_tint);
            painter.rect_stroke(front, s * 0.06, stroke_fg, egui::StrokeKind::Middle);
        }
        _ => {
            // Custom imported app: Hexagon / cube package emblem
            let r = s * 0.24;
            painter.rect_stroke(
                Rect::from_center_size(c, Vec2::splat(r * 1.65)),
                s * 0.08,
                stroke_fg,
                egui::StrokeKind::Middle,
            );
            painter.circle_filled(c, (s * 0.09).max(3.0), base);
        }
    }
}
