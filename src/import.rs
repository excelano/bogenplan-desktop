// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::path::Path;

use bogenplan_store::{Calendar, CalendarId, Import, Imported, OccurrenceTime, Reply};
use chrono::{Duration, Local};
use eframe::egui::{self, Align, Layout, RichText, ScrollArea, Ui};

use crate::display::Readout;
use crate::editor::{Pending, calendar_field};
use crate::glyph::Label;
use crate::palette;

const LARGEST: u64 = 16 * 1024 * 1024;

pub struct Importing {
    file: String,
    read: Result<Import, String>,
    calendar: Option<CalendarId>,
    pending: Pending,
}

pub enum Outcome {
    Open,
    Closed,
    Import(Import, Option<CalendarId>),
}

impl Importing {
    pub fn open(path: &Path) -> Importing {
        Importing {
            file: path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            read: read(path),
            calendar: None,
            pending: Pending::new(),
        }
    }

    pub fn uids(&self) -> Vec<String> {
        self.read.as_ref().map_or_else(
            |_| Vec::new(),
            |import| {
                import
                    .events
                    .iter()
                    .map(|event| event.uid.clone())
                    .collect()
            },
        )
    }

    pub fn wait(&mut self, reply: Reply) {
        self.pending.wait(reply);
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        calendars: &[Calendar],
        default: Option<&Calendar>,
        held: &[Option<CalendarId>],
        readout: &Readout,
    ) -> Outcome {
        if self.pending.poll(ui) {
            return Outcome::Closed;
        }
        ui.set_width(380.0);
        let import = match &self.read {
            Ok(import) => import,
            Err(error) => {
                heading(ui, "Not Added", &self.file);
                ui.label(error);
                ui.add_space(8.0);
                let mut outcome = Outcome::Open;
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        outcome = Outcome::Closed;
                    }
                });
                return outcome;
            }
        };
        let writable = |id: &CalendarId| {
            calendars
                .iter()
                .any(|calendar| &calendar.id == id && !calendar.read_only)
        };
        if !self.calendar.as_ref().is_some_and(writable) {
            self.calendar = default.map(|calendar| calendar.id.clone());
        }
        let any_held = held.iter().any(Option::is_some);
        let all_held = held.iter().all(Option::is_some);
        let (title, action) = match (import.cancellation, all_held) {
            (true, _) => ("Cancelled", any_held.then_some("Remove")),
            (false, true) => ("Update Calendar", Some("Update")),
            (false, false) => ("Add to Calendar", Some("Add")),
        };
        heading(ui, title, &self.file);
        ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for (event, held) in import.events.iter().zip(held) {
                ui.add_space(6.0);
                summary(ui, event, readout);
                if let Some(held) = held {
                    let calendar = calendars
                        .iter()
                        .find(|calendar| &calendar.id == held)
                        .map_or("a calendar", |calendar| calendar.title.as_str());
                    let note = if import.cancellation {
                        format!("Your copy is in {calendar}.")
                    } else {
                        format!("Replaces the copy in {calendar}.")
                    };
                    ui.label(RichText::new(note).size(11.0).color(palette::LABEL));
                }
            }
        });
        ui.add_space(10.0);
        let needs_calendar = !import.cancellation && !all_held;
        if needs_calendar {
            ui.horizontal(|ui| {
                ui.label("Calendar");
                match self.calendar.as_mut() {
                    Some(chosen) => calendar_field(ui, calendars, chosen),
                    None => {
                        ui.label(
                            RichText::new("No calendar accepts new events.").color(palette::LABEL),
                        );
                    }
                }
            });
        }
        let footer = match (import.cancellation, any_held) {
            (true, true) => Some("The organizer cancelled this event."),
            (true, false) => {
                Some("The organizer cancelled this event, and it is not in your calendars.")
            }
            (false, _) if import.invitation => {
                Some("Saved as your own copy. Bogenplan does not reply to the organizer.")
            }
            (false, _) => None,
        };
        if let Some(footer) = footer {
            ui.label(RichText::new(footer).size(11.0).color(palette::LABEL));
        }
        self.pending.show(ui);
        ui.add_space(8.0);
        let mut outcome = Outcome::Open;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let Some(action) = action {
                let ready = !self.pending.busy() && (!needs_calendar || self.calendar.is_some());
                if ui.add_enabled(ready, egui::Button::new(action)).clicked() {
                    outcome = Outcome::Import(import.clone(), self.calendar.clone());
                }
            }
            let close = if action.is_some() { "Cancel" } else { "Close" };
            if ui.button(close).clicked() {
                outcome = Outcome::Closed;
            }
            if self.pending.busy() {
                ui.spinner();
            }
        });
        outcome
    }
}

fn read(path: &Path) -> Result<Import, String> {
    let size = std::fs::metadata(path)
        .map_err(|error| format!("The file could not be opened: {error}."))?
        .len();
    if size > LARGEST {
        return Err("The file is too large to be a calendar.".to_string());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("The file could not be read: {error}."))?;
    Import::read(&text)
}

pub fn heading(ui: &mut Ui, title: &str, file: &str) {
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(title).size(15.0).strong());
        ui.label(RichText::new(file).size(11.0).color(palette::LABEL));
    });
    ui.add_space(4.0);
}

fn summary(ui: &mut Ui, event: &Imported, readout: &Readout) {
    let title = if event.title.is_empty() {
        "Untitled"
    } else {
        event.title.as_str()
    };
    ui.label(
        RichText::new(title)
            .size(14.0)
            .strong()
            .color(palette::LINE),
    );
    match event.time {
        OccurrenceTime::AllDay { start, end } => {
            let first = start.format("%a %-d %b %Y").to_string();
            let last = (end - Duration::days(1)).format("%a %-d %b %Y").to_string();
            let dates = if first == last {
                format!("All day, {first}")
            } else {
                format!("All day, {first} – {last}")
            };
            ui.label(RichText::new(dates).size(12.0).color(palette::LABEL));
        }
        OccurrenceTime::Timed { start, end } => {
            ui.horizontal(|ui| {
                Label::of(&readout.position(start), 14.0, palette::LINE).show(ui);
                ui.label(
                    RichText::new(format!("for {}", readout.duration(start, end)))
                        .size(12.0)
                        .color(palette::LABEL),
                );
            });
            let when = if readout.shows_civil_time {
                readout.clock.date_time(start)
            } else {
                start
                    .with_timezone(&Local)
                    .format("%a %-d %b %Y")
                    .to_string()
            };
            ui.label(RichText::new(when).size(11.0).color(palette::LABEL));
        }
    }
    if event.repeats {
        ui.label(RichText::new("Repeats").size(11.0).color(palette::LABEL));
    }
}
