// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use eframe::egui::{
    Align2, Color32, FontId, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, pos2,
    vec2,
};

use crate::display::{Glyph, Position};

const DOT_RATIO: f32 = 2.6 / 9.0;

pub fn paint(painter: &Painter, glyph: Glyph, center: Pos2, radius: f32, color: Color32) {
    let stroke = Stroke::new(radius * 0.24, color);
    match glyph {
        Glyph::Noon | Glyph::Midnight => {
            painter.circle_stroke(center, radius, stroke);
            if glyph == Glyph::Noon {
                painter.circle_filled(center, radius * DOT_RATIO, color);
            }
        }
        Glyph::Sunrise | Glyph::Sunset => {
            let apex = if glyph == Glyph::Sunrise {
                -radius
            } else {
                radius
            };
            let points = vec![
                pos2(center.x, center.y + apex),
                pos2(center.x - radius, center.y - apex),
                pos2(center.x + radius, center.y - apex),
            ];
            painter.add(Shape::closed_line(points, stroke));
        }
    }
}

pub struct Label {
    pub glyph: Glyph,
    pub value: Option<String>,
    pub before: bool,
    pub font: FontId,
    pub color: Color32,
}

impl Label {
    pub fn new(
        glyph: Glyph,
        value: Option<String>,
        before: bool,
        size: f32,
        color: Color32,
    ) -> Label {
        Label {
            glyph,
            value,
            before,
            font: FontId::proportional(size),
            color,
        }
    }

    pub fn of(position: &Position, size: f32, color: Color32) -> Label {
        Label::new(
            position.glyph,
            position.value(),
            position.before,
            size,
            color,
        )
    }

    fn radius(&self) -> f32 {
        self.font.size * 0.41
    }

    pub fn size(&self, painter: &Painter) -> Vec2 {
        let glyph = 2.0 * self.radius() + self.radius() * 0.24;
        let text = self.value.as_ref().map_or(Vec2::ZERO, |value| {
            painter
                .layout_no_wrap(value.clone(), self.font.clone(), self.color)
                .size()
        });
        let gap = if self.value.is_some() {
            self.radius() * 0.6
        } else {
            0.0
        };
        vec2(glyph + gap + text.x, text.y.max(glyph))
    }

    pub fn paint(&self, painter: &Painter, left_center: Pos2) -> Rect {
        let size = self.size(painter);
        let radius = self.radius();
        let glyph_width = 2.0 * radius + radius * 0.24;
        let gap = if self.value.is_some() {
            radius * 0.6
        } else {
            0.0
        };
        let mut x = left_center.x;
        if self.before
            && let Some(value) = &self.value
        {
            let drawn = painter.text(
                pos2(x, left_center.y),
                Align2::LEFT_CENTER,
                value,
                self.font.clone(),
                self.color,
            );
            x = drawn.right() + gap;
        }
        paint(
            painter,
            self.glyph,
            pos2(x + glyph_width / 2.0, left_center.y),
            radius,
            self.color,
        );
        x += glyph_width + gap;
        if !self.before
            && let Some(value) = &self.value
        {
            painter.text(
                pos2(x, left_center.y),
                Align2::LEFT_CENTER,
                value,
                self.font.clone(),
                self.color,
            );
        }
        Rect::from_min_size(pos2(left_center.x, left_center.y - size.y / 2.0), size)
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let size = self.size(ui.painter());
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        self.paint(ui.painter(), rect.left_center());
        response
    }
}
