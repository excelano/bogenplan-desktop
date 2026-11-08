// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use bogenplan_store::Occurrence;
use eframe::egui::{CornerRadius, FontId, Rect, Sense, Ui, pos2, vec2};

use crate::day::Colors;
use crate::palette::{self, with_opacity};

const ROW_HEIGHT: f32 = 22.0;
const SPACING: f32 = 6.0;
const ROW_LIMIT: usize = 2;
const FONT_SIZE: f32 = 11.0;

pub enum Picked {
    One(Occurrence),
    All(Vec<Occurrence>),
}

pub fn strip(ui: &mut Ui, day: i64, events: &[Occurrence], colors: &Colors) -> Option<Picked> {
    if events.is_empty() {
        return None;
    }
    let width = ui.available_width();
    let painter = ui.painter().clone();
    let chip_width = |title: &str| {
        (painter
            .layout_no_wrap(
                title.to_string(),
                FontId::proportional(FONT_SIZE),
                palette::LINE,
            )
            .size()
            .x
            + 20.0)
            .min(width)
    };
    let more_width = |hidden: usize| chip_width(&format!("+{hidden} more"));
    let arrange = |shown: usize| -> Option<Vec<Rect>> {
        let mut widths: Vec<f32> = events[..shown]
            .iter()
            .map(|event| chip_width(&event.title))
            .collect();
        if shown < events.len() {
            widths.push(more_width(events.len() - shown));
        }
        let mut frames = Vec::new();
        let (mut x, mut row) = (0.0, 0);
        for chip in widths {
            if x > 0.0 && x + chip > width {
                x = 0.0;
                row += 1;
            }
            if row >= ROW_LIMIT {
                return None;
            }
            frames.push(Rect::from_min_size(
                pos2(x, row as f32 * (ROW_HEIGHT + 4.0)),
                vec2(chip, ROW_HEIGHT),
            ));
            x += chip + SPACING;
        }
        Some(frames)
    };
    let (shown, frames) = (0..=events.len())
        .rev()
        .find_map(|shown| arrange(shown).map(|frames| (shown, frames)))?;
    let height = frames
        .iter()
        .map(|frame| frame.bottom())
        .fold(0.0, f32::max);
    let (area, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let mut picked = None;
    for (i, frame) in frames.iter().enumerate() {
        let chip = frame.translate(area.min.to_vec2());
        let response = ui.interact(chip, ui.id().with(("all-day", day, i)), Sense::click());
        let (title, fill) = match events.get(i).filter(|_| i < shown) {
            Some(event) => (event.title.clone(), with_opacity(colors.of(event), 0.4)),
            None => (
                format!("+{} more", events.len() - shown),
                with_opacity(palette::LINE, 0.12),
            ),
        };
        let painter = ui.painter().with_clip_rect(chip);
        painter.rect_filled(chip, CornerRadius::same(11), fill);
        let galley = crate::day::text(
            &painter,
            &title,
            FONT_SIZE,
            palette::LINE,
            chip.width() - 20.0,
            1,
        );
        painter.galley(
            chip.left_center() + vec2(10.0, -galley.size().y / 2.0),
            galley,
            palette::LINE,
        );
        if response.clicked() {
            picked = Some(if i < shown {
                Picked::One(events[i].clone())
            } else {
                Picked::All(events.to_vec())
            });
        }
    }
    picked
}
