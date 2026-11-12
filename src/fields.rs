// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use chrono::{Datelike, NaiveDate};
use eframe::egui::{Button, DragValue, Ui};

use crate::civil::Clock;

pub fn date(ui: &mut Ui, id: &str, date: &mut NaiveDate) {
    let mut picked = jiff::civil::date(date.year() as i16, date.month() as i8, date.day() as i8);
    let before = picked;
    ui.add(
        egui_extras::DatePickerButton::new(&mut picked)
            .id_salt(id)
            .format("%a %-d %b %Y")
            .calendar_week(false)
            .show_icon(false),
    );
    if picked != before
        && let Some(chosen) = NaiveDate::from_ymd_opt(
            i32::from(picked.year()),
            picked.month() as u32,
            picked.day() as u32,
        )
    {
        *date = chosen;
    }
}

pub fn time(ui: &mut Ui, clock: Clock, hour: &mut u32, minute: &mut u32) -> bool {
    let mut changed = false;
    match clock {
        Clock::TwentyFourHour => {
            changed |= ui
                .add(
                    DragValue::new(hour)
                        .range(0..=23)
                        .custom_formatter(|n, _| format!("{n:02}")),
                )
                .changed();
        }
        Clock::TwelveHour => {
            let mut shown = (*hour + 11) % 12 + 1;
            if ui.add(DragValue::new(&mut shown).range(1..=12)).changed() {
                *hour = shown % 12 + if *hour >= 12 { 12 } else { 0 };
                changed = true;
            }
        }
    }
    ui.label(":");
    changed |= ui
        .add(
            DragValue::new(minute)
                .range(0..=59)
                .custom_formatter(|n, _| format!("{n:02}")),
        )
        .changed();
    if clock == Clock::TwelveHour {
        let meridiem = if *hour >= 12 { "PM" } else { "AM" };
        if ui
            .add(Button::new(meridiem))
            .on_hover_text("Switch AM and PM")
            .clicked()
        {
            *hour = (*hour + 12) % 24;
            changed = true;
        }
    }
    changed
}
