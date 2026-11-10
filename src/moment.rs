// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use bogenplan_store::{Calendar, Reply};
use chrono::{DateTime, Local, NaiveTime, TimeZone, Timelike, Utc};
use eframe::egui::{
    self, Align, CornerRadius, Layout, RichText, Sense, Stroke, TextEdit, Ui, vec2,
};

use crate::axis::{DayAxis, LENGTH};
use crate::civil::Clock;
use crate::display::{Readout, Scale};
use crate::entry;
use crate::fields;
use crate::glyph::Label;
use crate::palette::{self, with_opacity};
use crate::time::seconds;

const POINTS_PER_STEP: f32 = 10.0;
const DIAL_HEIGHT: f32 = 36.0;

pub struct Draft {
    pub axis: DayAxis,
    pub instant: DateTime<Utc>,
    title: String,
    sets_civil_time: bool,
    pending: Option<Reply>,
    failure: Option<String>,
}

pub enum Outcome {
    Open,
    Cancelled,
    Add { at: DateTime<Utc>, title: String },
    Added,
}

impl Draft {
    pub fn new(at: DateTime<Utc>, axis: DayAxis, readout: &Readout) -> Draft {
        Draft {
            axis,
            instant: stepped(at, 0, &axis, readout),
            title: String::new(),
            sets_civil_time: false,
            pending: None,
            failure: None,
        }
    }

    pub fn wait(&mut self, reply: Reply) {
        self.failure = None;
        self.pending = Some(reply);
    }

    pub fn show(&mut self, ui: &mut Ui, readout: &Readout, calendar: Option<&Calendar>) -> Outcome {
        if let Some(reply) = &self.pending {
            match reply.try_recv() {
                Ok(Ok(())) => return Outcome::Added,
                Ok(Err(error)) => {
                    self.failure = Some(error);
                    self.pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ui
                    .ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.failure = Some("The calendar did not answer.".to_string());
                    self.pending = None;
                }
            }
        }
        ui.set_width(360.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("Set a Moment").size(15.0).strong());
            ui.add_space(10.0);
            Label::of(&readout.position(self.instant), 28.0, palette::LINE).show(ui);
            if readout.shows_civil_time {
                ui.label(
                    RichText::new(readout.clock.date_time(self.instant))
                        .size(12.0)
                        .color(palette::LABEL),
                );
            }
        });
        ui.add_space(10.0);
        if readout.shows_civil_time {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.sets_civil_time, false, "Bogenzeit");
                ui.selectable_value(&mut self.sets_civil_time, true, "Civil time");
            });
            ui.add_space(6.0);
        }
        if self.sets_civil_time && readout.shows_civil_time {
            self.civil(ui, readout.clock);
        } else {
            entry::entry(ui, "moment", &mut self.instant, &self.axis, readout);
            ui.add_space(4.0);
            dial(ui, "moment dial", &mut self.instant, &self.axis, readout);
            self.keys(ui, readout);
        }
        ui.add_space(10.0);
        let default_title = capitalized(&readout.spoken(&readout.position(self.instant)));
        ui.add(
            TextEdit::singleline(&mut self.title)
                .hint_text(default_title.as_str())
                .desired_width(f32::INFINITY),
        );
        let footer = match calendar {
            Some(calendar) => format!(
                "Added to {} with an alert at this moment, fixed for where you are now.",
                calendar.title
            ),
            None => "There is no calendar for new moments. Choose one in Settings.".to_string(),
        };
        ui.label(RichText::new(footer).size(11.0).color(palette::LABEL));
        if let Some(failure) = &self.failure {
            ui.label(
                RichText::new(format!("Not added: {failure}"))
                    .size(12.0)
                    .color(egui::Color32::from_rgb(0xFF, 0x8A, 0x80)),
            );
        }
        ui.add_space(8.0);
        let mut outcome = Outcome::Open;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let can_add = calendar.is_some() && self.pending.is_none();
            if ui.add_enabled(can_add, egui::Button::new("Add")).clicked()
                || (can_add && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                let name = self.title.trim();
                let title = if name.is_empty() {
                    default_title.clone()
                } else {
                    name.to_string()
                };
                outcome = Outcome::Add {
                    at: self.instant,
                    title,
                };
            }
            if ui.button("Cancel").clicked() {
                outcome = Outcome::Cancelled;
            }
            if self.pending.is_some() {
                ui.spinner();
            }
        });
        outcome
    }

    fn keys(&mut self, ui: &Ui, readout: &Readout) {
        if ui.memory(|memory| memory.focused().is_some()) {
            return;
        }
        let (earlier, later) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
            )
        });
        if earlier {
            self.step(-1, readout);
        }
        if later {
            self.step(1, readout);
        }
    }

    fn civil(&mut self, ui: &mut Ui, clock: Clock) {
        let local = self.instant.with_timezone(&Local);
        let (mut hour, mut minute) = (local.hour(), local.minute());
        ui.horizontal(|ui| {
            ui.label("Time");
            let changed = fields::time(ui, clock, &mut hour, &mut minute);
            if changed
                && let Some(time) = NaiveTime::from_hms_opt(hour, minute, 0)
                && let Some(picked) = Local
                    .from_local_datetime(&local.date_naive().and_time(time))
                    .earliest()
            {
                self.instant = whole_second(
                    picked
                        .with_timezone(&Utc)
                        .clamp(self.axis.start(), self.axis.end()),
                );
            }
        });
    }

    fn step(&mut self, steps: i64, readout: &Readout) {
        self.instant = stepped(self.instant, steps, &self.axis, readout);
    }
}

pub fn dial(
    ui: &mut Ui,
    id: &str,
    at: &mut DateTime<Utc>,
    axis: &DayAxis,
    readout: &Readout,
) -> bool {
    let before = *at;
    let id = ui.id().with(id);
    let row = vec2(ui.available_width(), DIAL_HEIGHT);
    ui.allocate_ui_with_layout(row, Layout::left_to_right(Align::Center), |ui| {
        if ui.button(" − ").on_hover_text("Earlier").clicked() {
            *at = stepped(*at, -1, axis, readout);
        }
        let width = ui.available_width() - 40.0;
        let (rect, response) = ui.allocate_exact_size(vec2(width, DIAL_HEIGHT), Sense::drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(
            rect,
            CornerRadius::same(18),
            with_opacity(palette::LINE, 0.08),
        );
        let ticks = (rect.width() / POINTS_PER_STEP) as usize;
        for i in 0..ticks {
            let x = rect.left() + (i as f32 + 0.5) * POINTS_PER_STEP;
            let long = i % 5 == 0;
            let half = if long { 9.0 } else { 5.0 };
            painter.vline(
                x,
                rect.center().y - half..=rect.center().y + half,
                Stroke::new(
                    1.0,
                    with_opacity(palette::LABEL, if long { 0.8 } else { 0.35 }),
                ),
            );
        }
        if response.drag_started() {
            ui.data_mut(|data| data.insert_temp(id, *at));
        }
        if let Some(start) = ui.data(|data| data.get_temp::<DateTime<Utc>>(id))
            && response.dragged()
        {
            let total = response.total_drag_delta().map_or(0.0, |delta| delta.x);
            *at = stepped(
                start,
                (total / POINTS_PER_STEP).round() as i64,
                axis,
                readout,
            );
        }
        if response.drag_stopped() {
            ui.data_mut(|data| data.remove::<DateTime<Utc>>(id));
        }
        if response.hovered() {
            let notches: i64 = ui.input(|i| {
                i.events
                    .iter()
                    .filter_map(|event| match event {
                        egui::Event::MouseWheel { delta, .. } => {
                            Some((delta.y + delta.x).signum() as i64)
                        }
                        _ => None,
                    })
                    .sum()
            });
            if notches != 0 {
                *at = stepped(*at, notches, axis, readout);
            }
        }
        if ui.button(" + ").on_hover_text("Later").clicked() {
            *at = stepped(*at, 1, axis, readout);
        }
    });
    *at != before
}

pub fn stepped(at: DateTime<Utc>, steps: i64, axis: &DayAxis, readout: &Readout) -> DateTime<Utc> {
    let reading = readout.reading(at);
    if readout.scale == Scale::Spans && reading.place_in_day().is_some() {
        let factor = 10f64.powi(readout.precision.decimals(Scale::Spans) as i32);
        let place = (axis.position(at) * factor).round() + steps as f64;
        let clamped = place.clamp(0.0, LENGTH * factor - 1.0);
        return whole_second(axis.instant_at(clamped / factor));
    }
    let step = 10f64.powi(3 - readout.precision.decimals(Scale::Kiloseconds) as i32);
    let anchor = at - seconds(reading.offset_s);
    let target = anchor + seconds(((reading.offset_s / step).round() + steps as f64) * step);
    whole_second(target.clamp(axis.start(), axis.end() - seconds(step)))
}

pub fn whole_second(at: DateTime<Utc>) -> DateTime<Utc> {
    let micros = at.timestamp_micros() + 1_000;
    let seconds = micros.div_euclid(1_000_000) + i64::from(micros.rem_euclid(1_000_000) != 0);
    DateTime::from_timestamp(seconds, 0).unwrap_or(at)
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::civil::Clock;
    use crate::display::Precision;
    use crate::place::Place;

    const HOUSTON: Place = Place {
        latitude: 29.76,
        longitude: -95.37,
    };

    fn readout(scale: Scale) -> Readout {
        Readout {
            place: HOUSTON,
            scale,
            precision: Precision::Low,
            shows_civil_time: true,
            clock: Clock::TwentyFourHour,
        }
    }

    #[test]
    fn a_step_is_one_displayed_span() {
        let axis = DayAxis::new(bz::Date::new(2026, 11, 4).unwrap(), HOUSTON);
        let readout = readout(Scale::Spans);
        let start = stepped(axis.noon(), 0, &axis, &readout);
        let next = stepped(start, 1, &axis, &readout);
        assert_eq!(readout.position(next).number.as_deref(), Some("1"));
        assert_eq!(next.timestamp_subsec_nanos(), 0);
    }

    #[test]
    fn a_kilosecond_step_at_low_precision_is_a_hundred_seconds() {
        let axis = DayAxis::new(bz::Date::new(2026, 11, 4).unwrap(), HOUSTON);
        let readout = readout(Scale::Kiloseconds);
        let start = stepped(axis.noon(), 0, &axis, &readout);
        assert_eq!(
            (stepped(start, 3, &axis, &readout) - start).num_seconds(),
            300
        );
    }

    #[test]
    fn whole_seconds_round_up() {
        let at = DateTime::from_timestamp(100, 400_000_000).unwrap();
        assert_eq!(whole_second(at).timestamp(), 101);
        assert_eq!(
            whole_second(DateTime::from_timestamp(100, 0).unwrap()).timestamp(),
            101
        );
    }
}
