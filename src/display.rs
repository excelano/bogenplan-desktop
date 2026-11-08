// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::civil::Clock;
use crate::place::Place;
use crate::time::instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scale {
    Spans,
    Kiloseconds,
}

impl Scale {
    pub const ALL: [Scale; 2] = [Scale::Spans, Scale::Kiloseconds];

    pub fn title(self) -> &'static str {
        match self {
            Scale::Spans => "Spans",
            Scale::Kiloseconds => "Kiloseconds",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Precision {
    Low,
    Medium,
    High,
}

impl Precision {
    pub const ALL: [Precision; 3] = [Precision::Low, Precision::Medium, Precision::High];

    pub fn title(self) -> &'static str {
        match self {
            Precision::Low => "Low",
            Precision::Medium => "Medium",
            Precision::High => "High",
        }
    }

    pub fn decimals(self, scale: Scale) -> u32 {
        let base = match self {
            Precision::Low => 0,
            Precision::Medium => 1,
            Precision::High => 2,
        };
        match scale {
            Scale::Spans => base,
            Scale::Kiloseconds => base + 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Noon,
    Midnight,
    Sunrise,
    Sunset,
}

impl Glyph {
    pub fn of(anchor: bz::Anchor) -> Glyph {
        match anchor {
            bz::Anchor::Noon => Glyph::Noon,
            bz::Anchor::Midnight => Glyph::Midnight,
        }
    }

    pub fn horizon_before(anchor: bz::Anchor) -> Glyph {
        match anchor {
            bz::Anchor::Noon => Glyph::Sunrise,
            bz::Anchor::Midnight => Glyph::Sunset,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Glyph::Noon => "solar noon",
            Glyph::Midnight => "solar midnight",
            Glyph::Sunrise => "sunrise",
            Glyph::Sunset => "sunset",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub glyph: Glyph,
    pub number: Option<String>,
    pub before: bool,
    pub unit: Option<&'static str>,
}

impl Position {
    pub fn of(reading: &bz::Reading, scale: Scale, precision: Precision) -> Position {
        if let (Scale::Spans, Some(span)) = (scale, reading.span()) {
            let decimals = precision.decimals(Scale::Spans);
            let factor = 10i64.pow(decimals);
            let steps = (span * factor as f64).floor() as i64;
            let horizon = steps == -100 * factor;
            return Position {
                glyph: if horizon {
                    Glyph::horizon_before(reading.anchor)
                } else {
                    Glyph::of(reading.anchor)
                },
                number: (steps != 0 && !horizon).then(|| decimal(steps.abs(), decimals)),
                before: steps < 0,
                unit: None,
            };
        }
        let decimals = precision.decimals(Scale::Kiloseconds);
        let steps = (reading.offset_s / 10f64.powi(3 - decimals as i32)).floor() as i64;
        Position {
            glyph: Glyph::of(reading.anchor),
            number: (steps != 0).then(|| decimal(steps.abs(), decimals)),
            before: steps < 0,
            unit: Some("ks"),
        }
    }

    pub fn value(&self) -> Option<String> {
        self.number.as_ref().map(|number| match self.unit {
            Some(unit) => format!("{number} {unit}"),
            None => number.clone(),
        })
    }
}

fn decimal(steps: i64, decimals: u32) -> String {
    if decimals == 0 {
        return steps.to_string();
    }
    let factor = 10i64.pow(decimals);
    format!(
        "{}.{:0width$}",
        steps / factor,
        steps % factor,
        width = decimals as usize
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Readout {
    pub place: Place,
    pub scale: Scale,
    pub precision: Precision,
    pub shows_civil_time: bool,
    pub clock: Clock,
}

impl Readout {
    pub fn reading(&self, at: DateTime<Utc>) -> bz::Reading {
        bz::Reading::at(instant(at), self.place.latitude, self.place.longitude)
    }

    pub fn position(&self, at: DateTime<Utc>) -> Position {
        Position::of(&self.reading(at), self.scale, self.precision)
    }

    pub fn duration(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> String {
        let duration = bz::SolarDuration::between(
            instant(from),
            instant(to),
            self.place.latitude,
            self.place.longitude,
        );
        if let (Scale::Spans, Some(spans)) = (self.scale, duration.display_spans()) {
            return if spans.abs() == 1 {
                "1 span".to_string()
            } else {
                format!("{} spans", spans.abs())
            };
        }
        let steps = duration.display_ks_steps().abs();
        format!("{}.{} ks", steps / 10, steps % 10)
    }

    pub fn spoken(&self, position: &Position) -> String {
        let Some(number) = &position.number else {
            return position.glyph.name().to_string();
        };
        let unit = if position.unit.is_none() {
            "spans"
        } else {
            "kiloseconds"
        };
        let side = if position.before { "before" } else { "after" };
        format!("{number} {unit} {side} {}", position.glyph.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUSTON: Place = Place {
        latitude: 29.76,
        longitude: -95.37,
    };

    fn reading(text: &str) -> bz::Reading {
        let at: DateTime<Utc> = text.parse().unwrap();
        bz::Reading::at(instant(at), HOUSTON.latitude, HOUSTON.longitude)
    }

    #[test]
    fn spans_agree_with_the_engine_form() {
        for text in [
            "2026-11-04T16:20:00Z",
            "2026-11-04T23:59:00Z",
            "2026-11-05T09:00:00Z",
            "2026-06-21T18:00:00Z",
        ] {
            let reading = reading(text);
            let position = Position::of(&reading, Scale::Spans, Precision::Low);
            let glyph = match position.glyph {
                Glyph::Noon => '☉',
                Glyph::Midnight => '○',
                Glyph::Sunrise => '△',
                Glyph::Sunset => '▽',
            };
            let shown = match position.value() {
                None => glyph.to_string(),
                Some(value) if position.before => format!("{value} {glyph}"),
                Some(value) => format!("{glyph} {value}"),
            };
            assert_eq!(
                Some(shown),
                reading.form(bz::Form::Span, Some(bz::Precision::Low)),
                "{text}"
            );
        }
    }

    #[test]
    fn kiloseconds_carry_their_unit() {
        let position = Position::of(
            &reading("2026-11-04T16:20:00Z"),
            Scale::Kiloseconds,
            Precision::Medium,
        );
        assert_eq!(position.unit, Some("ks"));
        assert!(position.before);
        assert_eq!(
            position
                .number
                .as_deref()
                .map(|n| n.split('.').nth(1).unwrap().len()),
            Some(2)
        );
    }

    #[test]
    fn decimals_pad() {
        assert_eq!(decimal(1205, 1), "120.5");
        assert_eq!(decimal(7, 2), "0.07");
        assert_eq!(decimal(42, 0), "42");
    }
}
