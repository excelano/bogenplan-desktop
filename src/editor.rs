// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::sync::mpsc::TryRecvError;

use bogenplan_store::{Calendar, CalendarId, Change, Occurrence, OccurrenceTime, Reply, Scope};
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveTime, TimeZone, Timelike, Utc};
use eframe::egui::{self, Align, Layout, RichText, TextEdit, Ui};

use crate::axis::DayAxis;
use crate::civil::{Clock, date_of, day_of, naive};
use crate::display::Readout;
use crate::entry;
use crate::fields;
use crate::moment;
use crate::palette;

pub enum Edited {
    Open,
    Closed,
    Save(Change, Scope),
}

pub enum Deleted {
    Open,
    Closed,
    Delete(Scope),
}

struct Pending {
    reply: Option<Reply>,
    failure: Option<String>,
}

impl Pending {
    fn new() -> Pending {
        Pending {
            reply: None,
            failure: None,
        }
    }

    fn poll(&mut self, ui: &Ui) -> bool {
        let Some(reply) = &self.reply else {
            return false;
        };
        match reply.try_recv() {
            Ok(Ok(())) => return true,
            Ok(Err(error)) => {
                self.failure = Some(error);
                self.reply = None;
            }
            Err(TryRecvError::Empty) => ui
                .ctx()
                .request_repaint_after(std::time::Duration::from_millis(100)),
            Err(TryRecvError::Disconnected) => {
                self.failure = Some("The calendar did not answer.".to_string());
                self.reply = None;
            }
        }
        false
    }

    fn busy(&self) -> bool {
        self.reply.is_some()
    }

    fn show(&self, ui: &mut Ui) {
        if let Some(failure) = &self.failure {
            ui.label(RichText::new(failure).size(12.0).color(FAILURE));
        }
    }
}

const FAILURE: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x8A, 0x80);

pub struct Editor {
    pub occurrence: Occurrence,
    title: String,
    location: String,
    notes: String,
    all_day: bool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    sets_civil_time: bool,
    calendar: CalendarId,
    asking: bool,
    pending: Pending,
}

impl Editor {
    pub fn new(occurrence: Occurrence) -> Editor {
        let (all_day, start, end) = match occurrence.time {
            OccurrenceTime::AllDay { start, end } => (
                true,
                local_instant(start, 9, 0),
                local_instant((end - Duration::days(1)).max(start), 10, 0),
            ),
            OccurrenceTime::Timed { start, end } => (false, start, end),
        };
        Editor {
            title: occurrence.title.clone(),
            location: occurrence.location.clone().unwrap_or_default(),
            notes: occurrence.notes.clone().unwrap_or_default(),
            all_day,
            start,
            end,
            sets_civil_time: false,
            calendar: occurrence.calendar.clone(),
            asking: false,
            pending: Pending::new(),
            occurrence,
        }
    }

    pub fn wait(&mut self, reply: Reply) {
        self.pending.failure = None;
        self.pending.reply = Some(reply);
    }

    fn time(&self) -> Result<OccurrenceTime, String> {
        if self.all_day {
            let (start, end) = (local_date(self.start), local_date(self.end));
            if end < start {
                return Err("The event ends before it starts.".to_string());
            }
            return Ok(OccurrenceTime::AllDay {
                start,
                end: end + Duration::days(1),
            });
        }
        if self.end < self.start {
            return Err("The event ends before it starts.".to_string());
        }
        Ok(OccurrenceTime::Timed {
            start: self.start,
            end: self.end,
        })
    }

    fn change(&self) -> Result<Change, String> {
        if self.title.trim().is_empty() {
            return Err("The event needs a title.".to_string());
        }
        Ok(Change {
            title: self.title.clone(),
            location: self.location.clone(),
            notes: self.notes.clone(),
            time: self.time()?,
            calendar: self.calendar.clone(),
        })
    }

    pub fn show(&mut self, ui: &mut Ui, calendars: &[Calendar], readout: &Readout) -> Edited {
        if self.pending.poll(ui) {
            return Edited::Closed;
        }
        ui.set_width(420.0);
        ui.label(RichText::new("Edit Event").size(15.0).strong());
        ui.add_space(10.0);
        let editable = !self.pending.busy() && !self.asking;
        ui.add_enabled_ui(editable, |ui| {
            egui::Grid::new("edit event")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Title");
                    ui.add(TextEdit::singleline(&mut self.title).desired_width(f32::INFINITY));
                    ui.end_row();
                    ui.label("Location");
                    ui.add(TextEdit::singleline(&mut self.location).desired_width(f32::INFINITY));
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut self.all_day, "All day");
                    ui.end_row();
                    let timed = !self.all_day;
                    if timed && readout.shows_civil_time {
                        ui.label("");
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut self.sets_civil_time, false, "Bogenzeit");
                            ui.selectable_value(&mut self.sets_civil_time, true, "Civil time");
                        });
                        ui.end_row();
                    }
                    let bogenzeit = timed && !(self.sets_civil_time && readout.shows_civil_time);
                    let clock = timed.then_some(readout.clock);
                    for (title, id, at) in [
                        ("Starts", "start", &mut self.start),
                        ("Ends", "end", &mut self.end),
                    ] {
                        ui.label(title);
                        if bogenzeit {
                            bogenzeit_field(ui, id, at, readout);
                        } else {
                            civil_field(ui, id, at, clock);
                        }
                        ui.end_row();
                    }
                    ui.label("Calendar");
                    calendar_field(ui, calendars, &mut self.calendar);
                    ui.end_row();
                    ui.label("Notes");
                    ui.add(
                        TextEdit::multiline(&mut self.notes)
                            .desired_rows(4)
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();
                });
        });
        let validated = self.change();
        if let Err(problem) = &validated {
            ui.label(RichText::new(problem).size(12.0).color(palette::LABEL));
        }
        self.pending.show(ui);
        ui.add_space(8.0);
        let mut outcome = Edited::Open;
        let moving = self.calendar != self.occurrence.calendar;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if self.pending.busy() {
                ui.spinner();
                return;
            }
            let Ok(change) = validated else {
                if ui.button("Cancel").clicked() {
                    outcome = Edited::Closed;
                }
                return;
            };
            if self.asking {
                if ui.button("Change All Events").clicked() {
                    outcome = Edited::Save(change.clone(), Scope::All);
                }
                if !moving && ui.button("Change Only This Event").clicked() {
                    outcome = Edited::Save(change, Scope::One);
                }
                if ui.button("Back").clicked() {
                    self.asking = false;
                }
                return;
            }
            if ui.button("Save").clicked() {
                if self.occurrence.repeats() {
                    self.asking = true;
                } else {
                    outcome = Edited::Save(change, Scope::One);
                }
            }
            if ui.button("Cancel").clicked() {
                outcome = Edited::Closed;
            }
        });
        if self.asking {
            ui.label(
                RichText::new(if moving {
                    "This is a repeating event. Moving it to another calendar moves every occurrence."
                } else {
                    "This is a repeating event."
                })
                .size(11.0)
                .color(palette::LABEL),
            );
        }
        outcome
    }
}

pub struct Deletion {
    pub occurrence: Occurrence,
    pending: Pending,
}

impl Deletion {
    pub fn new(occurrence: Occurrence) -> Deletion {
        Deletion {
            occurrence,
            pending: Pending::new(),
        }
    }

    pub fn wait(&mut self, reply: Reply) {
        self.pending.failure = None;
        self.pending.reply = Some(reply);
    }

    pub fn show(&mut self, ui: &mut Ui) -> Deleted {
        if self.pending.poll(ui) {
            return Deleted::Closed;
        }
        ui.set_width(360.0);
        ui.label(RichText::new("Delete Event").size(15.0).strong());
        ui.add_space(8.0);
        ui.label(format!("Delete “{}”?", self.occurrence.title));
        if self.occurrence.repeats() {
            ui.label(
                RichText::new("This is a repeating event.")
                    .size(11.0)
                    .color(palette::LABEL),
            );
        }
        self.pending.show(ui);
        ui.add_space(8.0);
        let mut outcome = Deleted::Open;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if self.pending.busy() {
                ui.spinner();
                return;
            }
            if self.occurrence.repeats() {
                if ui.button("Delete All Events").clicked() {
                    outcome = Deleted::Delete(Scope::All);
                }
                if ui.button("Delete Only This Event").clicked() {
                    outcome = Deleted::Delete(Scope::One);
                }
            } else if ui.button("Delete").clicked() {
                outcome = Deleted::Delete(Scope::One);
            }
            if ui.button("Cancel").clicked() {
                outcome = Deleted::Closed;
            }
        });
        outcome
    }
}

fn bogenzeit_field(ui: &mut Ui, id: &str, at: &mut DateTime<Utc>, readout: &Readout) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            let axis = DayAxis::containing(*at, readout.place);
            let shown = naive(axis.solar.date);
            let mut date = shown;
            fields::date(ui, id, &mut date);
            if date != shown {
                let moved = DayAxis::new(date_of(day_of(date)), readout.place);
                *at = moved.instant_at(axis.position(*at));
            }
            if readout.shows_civil_time {
                ui.label(
                    RichText::new(readout.clock.date_time(*at))
                        .size(11.0)
                        .color(palette::LABEL),
                );
            }
        });
        let axis = DayAxis::containing(*at, readout.place);
        entry::entry(ui, id, at, &axis, readout);
        moment::dial(ui, id, at, &axis, readout);
    });
}

fn civil_field(ui: &mut Ui, id: &str, at: &mut DateTime<Utc>, clock: Option<Clock>) {
    let local = at.with_timezone(&Local);
    let (shown, mut hour, mut minute) = (local.date_naive(), local.hour(), local.minute());
    let mut date = shown;
    let mut changed = false;
    ui.horizontal(|ui| {
        fields::date(ui, id, &mut date);
        if let Some(clock) = clock {
            changed = fields::time(ui, clock, &mut hour, &mut minute);
        }
    });
    if changed || date != shown {
        *at = local_instant(date, hour, minute);
    }
}

fn local_date(at: DateTime<Utc>) -> NaiveDate {
    at.with_timezone(&Local).date_naive()
}

fn local_instant(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    let naive = date.and_time(NaiveTime::from_hms_opt(hour, minute, 0).unwrap_or_default());
    Local
        .from_local_datetime(&naive)
        .earliest()
        .unwrap_or_else(|| Local.from_utc_datetime(&naive))
        .with_timezone(&Utc)
}

fn calendar_field(ui: &mut Ui, calendars: &[Calendar], chosen: &mut CalendarId) {
    let title = calendars
        .iter()
        .find(|calendar| &calendar.id == chosen)
        .map_or("", |calendar| calendar.title.as_str());
    egui::ComboBox::from_id_salt("edit calendar")
        .selected_text(title)
        .show_ui(ui, |ui| {
            for calendar in calendars.iter().filter(|calendar| !calendar.read_only) {
                ui.selectable_value(chosen, calendar.id.clone(), &calendar.title);
            }
        });
}
