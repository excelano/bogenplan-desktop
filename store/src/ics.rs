// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use chrono::{Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use icalendar::parser::{Component, Property, read_calendar, unfold};
use rrule::Tz;

use crate::zones::{self, ZoneHint};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Stamp {
    Date(NaiveDate),
    Zoned(NaiveDateTime, Tz),
}

impl Stamp {
    pub fn to_utc(&self) -> chrono::DateTime<Utc> {
        match self {
            Stamp::Date(date) => {
                Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight exists"))
            }
            Stamp::Zoned(naive, zone) => zoned(*naive, *zone).with_timezone(&Utc),
        }
    }
}

pub(crate) fn zoned(naive: NaiveDateTime, zone: Tz) -> chrono::DateTime<Tz> {
    zone.from_local_datetime(&naive)
        .earliest()
        .or_else(|| {
            zone.from_local_datetime(&(naive + Duration::hours(1)))
                .earliest()
        })
        .unwrap_or_else(|| zone.from_utc_datetime(&naive))
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Event {
    pub uid: String,
    pub summary: String,
    pub location: Option<String>,
    pub description: Option<String>,
    pub start: Stamp,
    pub end: Option<Stamp>,
    pub duration: Option<Duration>,
    pub rrule: Option<String>,
    pub rdates: Vec<Stamp>,
    pub exdates: Vec<Stamp>,
    pub recurrence_id: Option<Stamp>,
}

pub(crate) fn parse(data: &str) -> Result<Vec<Event>, String> {
    let unfolded = unfold(data);
    let calendar = read_calendar(&unfolded)?;
    let mut components = Vec::new();
    collect(&calendar.components, &mut components);
    let hints: Vec<ZoneHint> = components
        .iter()
        .filter(|component| component.name.as_str() == "VTIMEZONE")
        .map(|component| zone_hint(component))
        .collect();
    Ok(components
        .iter()
        .filter(|component| component.name.as_str() == "VEVENT")
        .filter_map(|component| event(component, &hints))
        .collect())
}

pub(crate) fn calendar_name(data: &str) -> Option<String> {
    let unfolded = unfold(data);
    let calendar = read_calendar(&unfolded).ok()?;
    let name = calendar
        .properties
        .iter()
        .find(|prop| prop.name.as_str() == "X-WR-CALNAME")?;
    Some(name.val.clone().unescape_text().as_str().trim().to_string())
        .filter(|name| !name.is_empty())
}

fn collect<'a>(components: &'a [Component<'a>], into: &mut Vec<&'a Component<'a>>) {
    for component in components {
        if component.name.as_str() == "VCALENDAR" {
            collect(&component.components, into);
        } else {
            into.push(component);
        }
    }
}

fn zone_hint(component: &Component) -> ZoneHint {
    let standard_offset_s = component
        .components
        .iter()
        .find(|part| part.name.as_str() == "STANDARD")
        .or_else(|| component.components.first())
        .and_then(|part| part.find_prop("TZOFFSETTO"))
        .and_then(|prop| utc_offset(prop.val.as_str()));
    ZoneHint {
        tzid: component
            .find_prop("TZID")
            .map(|prop| prop.val.as_str().to_string())
            .unwrap_or_default(),
        location: component
            .find_prop("X-LIC-LOCATION")
            .map(|prop| prop.val.as_str().to_string()),
        standard_offset_s,
    }
}

fn utc_offset(value: &str) -> Option<i32> {
    let sign = match value.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let hours: i32 = value.get(1..3)?.parse().ok()?;
    let minutes: i32 = value.get(3..5)?.parse().ok()?;
    Some(sign * (hours * 3600 + minutes * 60))
}

fn event(component: &Component, hints: &[ZoneHint]) -> Option<Event> {
    let start = component
        .find_prop("DTSTART")
        .and_then(|prop| stamps(prop, hints).into_iter().next())?;
    let all = |name: &str| -> Vec<Stamp> {
        component
            .properties
            .iter()
            .filter(|prop| prop.name.as_str() == name)
            .flat_map(|prop| stamps(prop, hints))
            .collect()
    };
    Some(Event {
        uid: text(component, "UID").unwrap_or_default(),
        summary: text(component, "SUMMARY").unwrap_or_default(),
        location: text(component, "LOCATION").filter(|text| !text.is_empty()),
        description: text(component, "DESCRIPTION").filter(|text| !text.is_empty()),
        end: component
            .find_prop("DTEND")
            .and_then(|prop| stamps(prop, hints).into_iter().next()),
        duration: component
            .find_prop("DURATION")
            .and_then(|prop| duration(prop.val.as_str())),
        rrule: component
            .find_prop("RRULE")
            .map(|prop| prop.val.as_str().to_string()),
        rdates: all("RDATE"),
        exdates: all("EXDATE"),
        recurrence_id: component
            .find_prop("RECURRENCE-ID")
            .and_then(|prop| stamps(prop, hints).into_iter().next()),
        start,
    })
}

fn text(component: &Component, name: &str) -> Option<String> {
    component
        .find_prop(name)
        .map(|prop| prop.val.clone().unescape_text().as_str().trim().to_string())
}

fn param<'a>(prop: &'a Property, key: &str) -> Option<&'a str> {
    prop.params
        .iter()
        .find(|param| param.key.as_str().eq_ignore_ascii_case(key))
        .and_then(|param| param.val.as_ref())
        .map(|val| val.as_str())
}

fn stamps(prop: &Property, hints: &[ZoneHint]) -> Vec<Stamp> {
    let is_date = param(prop, "VALUE").is_some_and(|value| value.eq_ignore_ascii_case("DATE"));
    let zone = param(prop, "TZID").map(|tzid| Tz::Tz(zones::resolve(tzid, hints)));
    prop.val
        .as_str()
        .split(',')
        .filter_map(|value| stamp(value.trim(), is_date, zone))
        .collect()
}

fn stamp(value: &str, is_date: bool, zone: Option<Tz>) -> Option<Stamp> {
    if is_date || value.len() == 8 {
        return NaiveDate::parse_from_str(value, "%Y%m%d")
            .ok()
            .map(Stamp::Date);
    }
    let (value, utc) = match value.strip_suffix('Z').or_else(|| value.strip_suffix('z')) {
        Some(value) => (value, true),
        None => (value, false),
    };
    let naive = NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok()?;
    let zone = if utc {
        Tz::UTC
    } else {
        zone.unwrap_or(Tz::LOCAL)
    };
    Some(Stamp::Zoned(naive, zone))
}

fn duration(value: &str) -> Option<Duration> {
    let (sign, rest) = match value.as_bytes().first()? {
        b'-' => (-1, &value[1..]),
        b'+' => (1, &value[1..]),
        _ => (1, value),
    };
    let rest = rest.strip_prefix('P')?;
    let mut seconds = 0i64;
    let mut number = String::new();
    let mut in_time = false;
    for c in rest.chars() {
        match c {
            '0'..='9' => number.push(c),
            'T' => in_time = true,
            unit => {
                let n: i64 = number.parse().ok()?;
                number.clear();
                seconds += n * match (unit, in_time) {
                    ('W', false) => 7 * 86_400,
                    ('D', false) => 86_400,
                    ('H', true) => 3600,
                    ('M', true) => 60,
                    ('S', true) => 1,
                    _ => return None,
                };
            }
        }
    }
    Some(Duration::seconds(sign * seconds))
}

pub(crate) fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

pub(crate) fn fold(line: &str) -> String {
    let mut folded = String::new();
    let mut width = 0;
    for c in line.chars() {
        if width + c.len_utf8() > 75 {
            folded.push_str("\r\n ");
            width = 1;
        }
        folded.push(c);
        width += c.len_utf8();
    }
    folded
}

#[cfg(test)]
mod tests {
    use super::*;

    const OVERRIDDEN: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Choir\\, weekly\r\nDTSTART;TZID=America/Chicago:20260127T170000\r\nDTEND;TZID=America/Chicago:20260127T180000\r\nRRULE:FREQ=WEEKLY;UNTIL=20260409T045959Z;BYDAY=TU\r\nEXDATE;TZID=America/Chicago:20260203T170000,20260210T170000\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Choir\\, moved\r\nRECURRENCE-ID;TZID=America/Chicago:20260217T170000\r\nDTSTART;TZID=America/Chicago:20260218T150000\r\nDURATION:PT1H30M\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn a_series_and_its_override() {
        let events = parse(OVERRIDDEN).unwrap();
        assert_eq!(events.len(), 2);
        let chicago = Tz::Tz(chrono_tz::Tz::America__Chicago);
        let at = |s| NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S").unwrap();
        assert_eq!(events[0].summary, "Choir, weekly");
        assert_eq!(
            events[0].start,
            Stamp::Zoned(at("20260127T170000"), chicago)
        );
        assert_eq!(events[0].exdates.len(), 2);
        assert_eq!(
            events[0].rrule.as_deref(),
            Some("FREQ=WEEKLY;UNTIL=20260409T045959Z;BYDAY=TU")
        );
        assert_eq!(
            events[1].recurrence_id,
            Some(Stamp::Zoned(at("20260217T170000"), chicago))
        );
        assert_eq!(events[1].duration, Some(Duration::minutes(90)));
    }

    #[test]
    fn dates_and_utc() {
        assert_eq!(
            stamp("20261104", false, None),
            Some(Stamp::Date(NaiveDate::from_ymd_opt(2026, 11, 4).unwrap()))
        );
        let utc = stamp("20261104T162000Z", false, None).unwrap();
        assert_eq!(utc.to_utc().to_rfc3339(), "2026-11-04T16:20:00+00:00");
    }

    #[test]
    fn a_feed_names_itself() {
        let named = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nX-WR-CALNAME:US Holidays\\, federal\r\nEND:VCALENDAR\r\n";
        assert_eq!(
            calendar_name(named).as_deref(),
            Some("US Holidays, federal")
        );
        assert_eq!(calendar_name(OVERRIDDEN), None);
    }

    #[test]
    fn durations() {
        assert_eq!(duration("P1D"), Some(Duration::days(1)));
        assert_eq!(duration("PT15M"), Some(Duration::minutes(15)));
        assert_eq!(duration("P1W"), Some(Duration::weeks(1)));
        assert_eq!(duration("-PT1H"), Some(Duration::hours(-1)));
        assert_eq!(utc_offset("-055036"), Some(-(5 * 3600 + 50 * 60)));
    }
}
