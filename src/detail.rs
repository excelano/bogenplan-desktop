// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use bogenplan_store::{Calendar, Occurrence, OccurrenceTime};
use chrono::Duration;
use eframe::egui::{self, Grid, RichText, Ui};

use crate::display::Readout;
use crate::glyph::Label;
use crate::palette;

pub enum Action {
    Edit,
    Delete,
}

pub fn show(
    ui: &mut Ui,
    occurrence: &Occurrence,
    calendar: Option<&Calendar>,
    readout: &Readout,
) -> Option<Action> {
    ui.label(
        RichText::new(&occurrence.title)
            .size(18.0)
            .color(palette::LINE)
            .strong(),
    );
    if let Some(calendar) = calendar {
        ui.horizontal(|ui| {
            let (dot, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter()
                .circle_filled(dot.center(), 5.0, palette::calendar(calendar.color));
            ui.label(RichText::new(&calendar.title).color(palette::LABEL));
        });
    }
    ui.add_space(8.0);
    section(ui, "Time");
    match occurrence.time {
        OccurrenceTime::AllDay { start, end } => {
            let last = end - Duration::days(1);
            let first = start.format("%a %-d %b %Y").to_string();
            let final_day = last.format("%a %-d %b %Y").to_string();
            let dates = if first == final_day {
                first
            } else {
                format!("{first} – {final_day}")
            };
            Grid::new("time")
                .num_columns(2)
                .spacing([16.0, 6.0])
                .show(ui, |ui| {
                    ui.label("All day");
                    ui.label(dates);
                    ui.end_row();
                });
        }
        OccurrenceTime::Timed { start, end } => {
            Grid::new("time")
                .num_columns(2)
                .spacing([16.0, 6.0])
                .show(ui, |ui| {
                    for (title, at) in [("Starts", start), ("Ends", end)] {
                        ui.label(title);
                        ui.vertical(|ui| {
                            Label::of(&readout.position(at), 14.0, palette::LINE).show(ui);
                            if readout.shows_civil_time {
                                ui.label(
                                    RichText::new(readout.clock.date_time(at))
                                        .size(11.0)
                                        .color(palette::LABEL),
                                );
                            }
                        });
                        ui.end_row();
                    }
                    ui.label("Length");
                    ui.label(readout.duration(start, end));
                    ui.end_row();
                    if let Some(eighth) = readout.reading(start).eighth() {
                        ui.label("Starts in");
                        ui.label(eighth.name());
                        ui.end_row();
                    }
                });
        }
    }
    for (title, text) in [
        ("Location", &occurrence.location),
        ("Notes", &occurrence.notes),
    ] {
        if let Some(text) = text {
            ui.add_space(8.0);
            section(ui, title);
            ui.label(text);
        }
    }
    ui.add_space(12.0);
    let mut action = None;
    ui.horizontal(|ui| {
        if ui.button("Edit").clicked() {
            action = Some(Action::Edit);
        }
        if ui.button("Delete").clicked() {
            action = Some(Action::Delete);
        }
    });
    action
}

fn section(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).size(11.0).color(palette::LABEL));
}
