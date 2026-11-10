// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use chrono::{DateTime, Utc};
use eframe::egui::{self, Color32, CornerRadius, RichText, Sense, TextEdit, Ui, vec2};

use crate::axis::{DayAxis, LENGTH};
use crate::display::{Glyph, Readout, Scale};
use crate::glyph;
use crate::moment::whole_second;
use crate::palette::{self, with_opacity};
use crate::time::seconds;

const GLYPHS: [Glyph; 4] = [Glyph::Midnight, Glyph::Sunrise, Glyph::Noon, Glyph::Sunset];
const UNPLACED: Color32 = Color32::from_rgb(0xFF, 0x8A, 0x80);

#[derive(Clone)]
struct Typed {
    number: String,
    before: bool,
    glyph: Glyph,
    shown: DateTime<Utc>,
}

impl Typed {
    fn of(at: DateTime<Utc>, readout: &Readout) -> Typed {
        let position = readout.position(at);
        Typed {
            number: position.number.unwrap_or_else(|| "0".to_string()),
            before: position.before,
            glyph: position.glyph,
            shown: at,
        }
    }
}

pub fn entry(
    ui: &mut Ui,
    id: &str,
    at: &mut DateTime<Utc>,
    axis: &DayAxis,
    readout: &Readout,
) -> bool {
    let id = ui.id().with(id).with("entry");
    let mut typed = ui
        .data(|data| data.get_temp::<Typed>(id))
        .filter(|typed| typed.shown == *at)
        .unwrap_or_else(|| Typed::of(*at, readout));
    let spans = uses_spans(axis, readout);
    let placeable = target(&typed, spans, axis).is_some();
    let mut changed = false;
    ui.horizontal(|ui| {
        let field = TextEdit::singleline(&mut typed.number).desired_width(52.0);
        let field = if placeable {
            field
        } else {
            field.text_color(UNPLACED)
        };
        changed |= ui.add(field).changed();
        ui.label(
            RichText::new(if spans { "spans" } else { "ks" })
                .size(11.0)
                .color(palette::LABEL),
        );
        changed |= ui
            .selectable_value(&mut typed.before, true, "before")
            .changed();
        changed |= ui
            .selectable_value(&mut typed.before, false, "after")
            .changed();
        for glyph in GLYPHS {
            if glyph_button(ui, glyph, typed.glyph == glyph).clicked() && typed.glyph != glyph {
                typed.glyph = glyph;
                changed = true;
            }
        }
    });
    let mut moved = false;
    if changed && let Some(target) = target(&typed, spans, axis) {
        moved = target != *at;
        *at = target;
    }
    typed.shown = *at;
    ui.data_mut(|data| data.insert_temp(id, typed));
    moved
}

fn target(typed: &Typed, spans: bool, axis: &DayAxis) -> Option<DateTime<Utc>> {
    let number = typed.number.trim().parse::<f64>().ok()?;
    if !number.is_finite() || number < 0.0 {
        return None;
    }
    placed(number, typed.before, typed.glyph, spans, axis)
}

fn uses_spans(axis: &DayAxis, readout: &Readout) -> bool {
    readout.scale == Scale::Spans && axis.solar.polar().is_none()
}

fn placed(
    number: f64,
    before: bool,
    glyph: Glyph,
    spans: bool,
    axis: &DayAxis,
) -> Option<DateTime<Utc>> {
    let signed = if before { -number } else { number };
    let ending_midnight = glyph == Glyph::Midnight && before && number > 0.0;
    if spans {
        let anchor = match glyph {
            Glyph::Midnight if ending_midnight => LENGTH,
            Glyph::Midnight => 0.0,
            Glyph::Sunrise => LENGTH / 4.0,
            Glyph::Noon => LENGTH / 2.0,
            Glyph::Sunset => LENGTH * 3.0 / 4.0,
        };
        let place = anchor + signed;
        return (0.0..LENGTH)
            .contains(&place)
            .then(|| whole_second(axis.instant_at(place)));
    }
    let anchor = match glyph {
        Glyph::Midnight if ending_midnight => axis.end(),
        Glyph::Midnight => axis.start(),
        Glyph::Sunrise => axis.sunrise()?,
        Glyph::Noon => axis.noon(),
        Glyph::Sunset => axis.sunset()?,
    };
    let at = anchor + seconds(signed * 1000.0);
    axis.contains(at).then(|| whole_second(at))
}

fn glyph_button(ui: &mut Ui, glyph: Glyph, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(24.0, 22.0), Sense::click());
    let fill = if selected {
        ui.visuals().selection.bg_fill
    } else if response.hovered() {
        with_opacity(palette::LINE, 0.12)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius::same(4), fill);
    glyph::paint(ui.painter(), glyph, rect.center(), 6.0, palette::LINE);
    response.on_hover_text(glyph.name())
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
            precision: Precision::Medium,
            shows_civil_time: true,
            clock: Clock::TwentyFourHour,
        }
    }

    fn axis() -> DayAxis {
        DayAxis::new(bz::Date::new(2026, 11, 4).unwrap(), HOUSTON)
    }

    #[test]
    fn a_typed_span_reads_back_as_typed() {
        let readout = readout(Scale::Spans);
        for (number, before, glyph) in [
            (88.5, true, Glyph::Noon),
            (98.2, false, Glyph::Midnight),
            (12.3, true, Glyph::Midnight),
            (4.0, false, Glyph::Noon),
        ] {
            let at = placed(number, before, glyph, true, &axis()).unwrap();
            let position = readout.position(at);
            assert_eq!(
                position.number.as_deref(),
                Some(format!("{number:.1}").as_str())
            );
            assert_eq!((position.before, position.glyph), (before, glyph));
        }
    }

    #[test]
    fn a_span_from_a_horizon_counts_from_the_quarter_mark() {
        let readout = readout(Scale::Spans);
        let at = placed(5.0, false, Glyph::Sunrise, true, &axis()).unwrap();
        let position = readout.position(at);
        assert_eq!(position.number.as_deref(), Some("95.0"));
        assert_eq!((position.before, position.glyph), (true, Glyph::Noon));
    }

    #[test]
    fn a_typed_offset_reads_back_as_typed() {
        let readout = readout(Scale::Kiloseconds);
        let at = placed(11.77, true, Glyph::Noon, false, &axis()).unwrap();
        let position = readout.position(at);
        assert_eq!(position.number.as_deref(), Some("11.77"));
        assert_eq!((position.before, position.glyph), (true, Glyph::Noon));
    }

    #[test]
    fn a_place_outside_the_day_is_refused() {
        assert!(placed(250.0, false, Glyph::Noon, true, &axis()).is_none());
        assert!(placed(0.0, true, Glyph::Midnight, true, &axis()).is_some());
    }
}
