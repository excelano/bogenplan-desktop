// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use chrono::{DateTime, Utc};

use crate::display::Glyph;
use crate::place::Place;
use crate::time::{instant, seconds, utc};

pub const LENGTH: f64 = 400.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub position: f64,
    pub instant: DateTime<Utc>,
    pub glyph: Glyph,
    pub number: Option<String>,
    pub before: bool,
    pub eighth_boundary: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct DayAxis {
    pub solar: bz::SolarDay,
    pub place: Place,
}

impl DayAxis {
    pub fn new(date: bz::Date, place: Place) -> DayAxis {
        DayAxis {
            solar: bz::SolarDay::compute(date, place.latitude, place.longitude),
            place,
        }
    }

    pub fn start(&self) -> DateTime<Utc> {
        utc(self.solar.midnight_start)
    }

    pub fn end(&self) -> DateTime<Utc> {
        utc(self.solar.midnight_end)
    }

    pub fn noon(&self) -> DateTime<Utc> {
        utc(self.solar.noon)
    }

    pub fn sunrise(&self) -> Option<DateTime<Utc>> {
        self.solar.sunrise().map(utc)
    }

    pub fn sunset(&self) -> Option<DateTime<Utc>> {
        self.solar.sunset().map(utc)
    }

    pub fn contains(&self, at: DateTime<Utc>) -> bool {
        self.start() <= at && at < self.end()
    }

    pub fn position(&self, at: DateTime<Utc>) -> f64 {
        if at <= self.start() {
            return 0.0;
        }
        if at >= self.end() {
            return LENGTH;
        }
        let reading = bz::Reading::at(instant(at), self.place.latitude, self.place.longitude);
        reading.place_in_day().unwrap_or_else(|| {
            LENGTH * (at - self.start()).as_seconds_f64()
                / (self.end() - self.start()).as_seconds_f64()
        })
    }

    pub fn instant_at(&self, position: f64) -> DateTime<Utc> {
        let position = position.clamp(0.0, LENGTH);
        match self.solar.instant_at(position) {
            Some(at) => utc(at),
            None => {
                self.start()
                    + seconds((self.end() - self.start()).as_seconds_f64() * position / LENGTH)
            }
        }
    }

    pub fn extent(&self, arc: bz::HalfArc) -> Option<(f64, f64)> {
        match arc {
            bz::HalfArc::AlwaysAbove => Some((0.0, LENGTH)),
            bz::HalfArc::AlwaysBelow => None,
            bz::HalfArc::Seconds(half) => Some((
                self.position(self.noon() - seconds(half)),
                self.position(self.noon() + seconds(half)),
            )),
        }
    }

    pub fn marks(&self, step: usize) -> Vec<Mark> {
        let (Some(sunrise), Some(sunset), Some(half_day)) =
            (self.sunrise(), self.sunset(), self.solar.half_day_s())
        else {
            return vec![
                self.plain(0.0, self.start(), Glyph::Midnight),
                self.plain(self.position(self.noon()), self.noon(), Glyph::Noon),
                self.plain(LENGTH, self.end(), Glyph::Midnight),
            ];
        };
        (0..=LENGTH as usize)
            .step_by(step)
            .map(|span| {
                let mut mark = self.mark(span, sunrise, sunset, half_day);
                mark.eighth_boundary = span % 50 == 0;
                mark
            })
            .collect()
    }

    fn plain(&self, position: f64, at: DateTime<Utc>, glyph: Glyph) -> Mark {
        Mark {
            position,
            instant: at,
            glyph,
            number: None,
            before: false,
            eighth_boundary: true,
        }
    }

    fn mark(
        &self,
        span: usize,
        sunrise: DateTime<Utc>,
        sunset: DateTime<Utc>,
        half_day: f64,
    ) -> Mark {
        let position = span as f64;
        let numbered = |at, glyph, number: i64, before| Mark {
            position,
            instant: at,
            glyph,
            number: Some(number.abs().to_string()),
            before,
            eighth_boundary: false,
        };
        match span {
            0 => self.plain(position, self.start(), Glyph::Midnight),
            1..100 => {
                let at = self.start()
                    + seconds((sunrise - self.start()).as_seconds_f64() * position / 100.0);
                numbered(at, Glyph::Midnight, span as i64, false)
            }
            100 => self.plain(position, sunrise, Glyph::Sunrise),
            101..300 => {
                let from_noon = span as i64 - 200;
                let at = self.noon() + seconds(half_day * from_noon as f64 / 100.0);
                if from_noon == 0 {
                    self.plain(position, at, Glyph::Noon)
                } else {
                    numbered(at, Glyph::Noon, from_noon, from_noon < 0)
                }
            }
            300 => self.plain(position, sunset, Glyph::Sunset),
            301..400 => {
                let at = sunset
                    + seconds((self.end() - sunset).as_seconds_f64() * (position - 300.0) / 100.0);
                numbered(at, Glyph::Midnight, 400 - span as i64, true)
            }
            _ => self.plain(position, self.end(), Glyph::Midnight),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUSTON: Place = Place {
        latitude: 29.76,
        longitude: -95.37,
    };

    fn axis() -> DayAxis {
        DayAxis::new(bz::Date::new(2026, 11, 4).unwrap(), HOUSTON)
    }

    #[test]
    fn anchors_sit_at_fixed_heights() {
        let axis = axis();
        assert!((axis.position(axis.noon()) - 200.0).abs() < 1e-6);
        assert!((axis.position(axis.sunrise().unwrap()) - 100.0).abs() < 1e-6);
        assert!((axis.position(axis.sunset().unwrap()) - 300.0).abs() < 1e-6);
    }

    #[test]
    fn position_and_instant_invert() {
        let axis = axis();
        for position in [12.5, 99.0, 150.25, 287.0, 333.3] {
            let back = axis.position(axis.instant_at(position));
            assert!(
                (back - position).abs() < 1e-4,
                "{position} came back as {back}"
            );
        }
    }

    #[test]
    fn marks_name_the_positional_side() {
        let marks = axis().marks(50);
        let labels: Vec<_> = marks
            .iter()
            .map(|m| (m.position as i64, m.glyph, m.number.clone(), m.before))
            .collect();
        assert_eq!(labels[1], (50, Glyph::Midnight, Some("50".into()), false));
        assert_eq!(labels[3], (150, Glyph::Noon, Some("50".into()), true));
        assert_eq!(labels[4], (200, Glyph::Noon, None, false));
        assert_eq!(labels[7], (350, Glyph::Midnight, Some("50".into()), true));
        assert!(marks.iter().all(|m| m.eighth_boundary));
    }
}
