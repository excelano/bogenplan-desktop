// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use chrono::Weekday;
use eframe::egui::{Align2, Button, FontId, Rect, Sense, Ui, pos2, vec2};

use crate::civil::{date_of, day_of, naive, week_start};
use crate::palette;

pub fn strip(ui: &mut Ui, selection: i64, today: i64, first: Weekday) -> Option<i64> {
    let start = day_of(week_start(naive(date_of(selection)), first));
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let arrow = vec2(28.0, 56.0);
        let width = (ui.available_width() - 2.0 * arrow.x) / 7.0;
        if ui
            .add_sized(arrow, Button::new("‹").frame(false))
            .on_hover_text("Previous week")
            .clicked()
        {
            picked = Some(selection - 7);
        }
        for day in start..start + 7 {
            let (cell, response) = ui.allocate_exact_size(vec2(width, 56.0), Sense::click());
            paint_cell(ui, cell, day, day == selection, day == today);
            if response.clicked() {
                picked = Some(day);
            }
        }
        if ui
            .add_sized(arrow, Button::new("›").frame(false))
            .on_hover_text("Next week")
            .clicked()
        {
            picked = Some(selection + 7);
        }
    });
    picked
}

fn paint_cell(ui: &Ui, cell: Rect, day: i64, selected: bool, today: bool) {
    let painter = ui.painter();
    let date = naive(date_of(day));
    let letter = date.format("%a").to_string();
    painter.text(
        pos2(cell.center().x, cell.top() + 8.0),
        Align2::CENTER_CENTER,
        &letter[..1],
        FontId::proportional(10.0),
        palette::LABEL,
    );
    let center = pos2(cell.center().x, cell.top() + 34.0);
    if selected {
        painter.circle_filled(
            center,
            17.0,
            if today { palette::SUN } else { palette::LINE },
        );
    }
    let color = if selected {
        palette::NIGHT
    } else if today {
        palette::SUN
    } else {
        palette::LINE
    };
    painter.text(
        center,
        Align2::CENTER_CENTER,
        date.format("%-d").to_string(),
        FontId::proportional(15.0),
        color,
    );
}
