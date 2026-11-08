// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use chrono::{DateTime, Duration, Utc};
use rrule::{RRule, RRuleSet, Tz, Unvalidated};

use crate::ics::{Event, Stamp, zoned};
use crate::model::OccurrenceTime;

pub(crate) struct Instance<'a> {
    pub event: &'a Event,
    pub time: OccurrenceTime,
}

pub(crate) fn instances<'a>(
    events: &'a [Event],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Vec<Instance<'a>> {
    let overrides: Vec<&Event> = events
        .iter()
        .filter(|event| event.recurrence_id.is_some())
        .collect();
    let mut found = Vec::new();
    for master in events.iter().filter(|event| event.recurrence_id.is_none()) {
        let replaced: Vec<DateTime<Utc>> = overrides
            .iter()
            .filter(|event| event.uid == master.uid)
            .filter_map(|event| event.recurrence_id.as_ref().map(Stamp::to_utc))
            .collect();
        for time in series(master, from, to) {
            if !replaced.contains(&time.start_instant()) && time.overlaps(from, to) {
                found.push(Instance {
                    event: master,
                    time,
                });
            }
        }
    }
    for event in overrides {
        let time = single(event);
        if time.overlaps(from, to) {
            found.push(Instance { event, time });
        }
    }
    found
}

fn length(event: &Event) -> Duration {
    match (&event.start, &event.end, event.duration) {
        (_, _, Some(duration)) => duration.max(Duration::zero()),
        (Stamp::Date(start), Some(Stamp::Date(end)), None) => {
            (*end - *start).max(Duration::days(1))
        }
        (Stamp::Date(_), _, None) => Duration::days(1),
        (start, Some(end), None) => (end.to_utc() - start.to_utc()).max(Duration::zero()),
        (_, None, None) => Duration::zero(),
    }
}

fn single(event: &Event) -> OccurrenceTime {
    occurrence_at(&event.start, length(event))
}

fn occurrence_at(start: &Stamp, length: Duration) -> OccurrenceTime {
    match start {
        Stamp::Date(date) => OccurrenceTime::AllDay {
            start: *date,
            end: *date + Duration::days(length.num_days().max(1)),
        },
        Stamp::Zoned(..) => {
            let start = start.to_utc();
            OccurrenceTime::Timed {
                start,
                end: start + length,
            }
        }
    }
}

fn series(event: &Event, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<OccurrenceTime> {
    if event.rrule.is_none() && event.rdates.is_empty() {
        return vec![single(event)];
    }
    let length = length(event);
    let Some(set) = rule_set(event) else {
        return vec![single(event)];
    };
    let earliest = from - length - Duration::days(1);
    let mut times = Vec::new();
    for at in &set.limit() {
        let start_utc = at.with_timezone(&Utc);
        if start_utc >= to {
            break;
        }
        if start_utc < earliest {
            continue;
        }
        let start = match event.start {
            Stamp::Date(_) => Stamp::Date(at.date_naive()),
            Stamp::Zoned(..) => Stamp::Zoned(start_utc.naive_utc(), Tz::UTC),
        };
        times.push(occurrence_at(&start, length));
    }
    times
}

fn rule_set(event: &Event) -> Option<RRuleSet> {
    let zone = match event.start {
        Stamp::Date(_) => Tz::UTC,
        Stamp::Zoned(_, zone) => zone,
    };
    let in_zone = |stamp: &Stamp| -> DateTime<Tz> {
        match stamp {
            Stamp::Date(date) => zoned(date.and_hms_opt(0, 0, 0).expect("midnight exists"), zone),
            Stamp::Zoned(naive, stamp_zone) => zoned(*naive, *stamp_zone).with_timezone(&zone),
        }
    };
    let start = in_zone(&event.start);
    let mut set = RRuleSet::new(start);
    if let Some(text) = &event.rrule {
        let rule: RRule<Unvalidated> = with_utc_until(text, zone).parse().ok()?;
        set = set.rrule(rule.validate(start).ok()?);
    }
    for rdate in &event.rdates {
        set = set.rdate(in_zone(rdate));
    }
    for exdate in &event.exdates {
        set = set.exdate(in_zone(exdate));
    }
    Some(set)
}

fn with_utc_until(rule: &str, zone: Tz) -> String {
    rule.split(';')
        .map(|part| match part.split_once('=') {
            Some((key, value)) if key.eq_ignore_ascii_case("UNTIL") && !value.ends_with('Z') => {
                let local = if value.len() == 8 {
                    format!("{value}T235959")
                } else {
                    value.to_string()
                };
                chrono::NaiveDateTime::parse_from_str(&local, "%Y%m%dT%H%M%S").map_or_else(
                    |_| part.to_string(),
                    |naive| {
                        format!(
                            "UNTIL={}",
                            zoned(naive, zone)
                                .with_timezone(&Utc)
                                .format("%Y%m%dT%H%M%SZ")
                        )
                    },
                )
            }
            _ => part.to_string(),
        })
        .collect::<Vec<_>>()
        .join(";")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ics::parse;

    fn utc(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn starts(data: &str, from: &str, to: &str) -> Vec<String> {
        let events = parse(data).unwrap();
        let mut found: Vec<String> = instances(&events, utc(from), utc(to))
            .iter()
            .map(|instance| match instance.time {
                OccurrenceTime::Timed { start, end } => {
                    format!(
                        "{} {} {}",
                        start.format("%m-%d %H:%M"),
                        (end - start).num_minutes(),
                        instance.event.summary
                    )
                }
                OccurrenceTime::AllDay { start, end } => {
                    format!("{start}..{end} {}", instance.event.summary)
                }
            })
            .collect();
        found.sort();
        found
    }

    const WEEKLY: &str = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Choir\r\nDTSTART;TZID=America/Chicago:20260127T170000\r\nDTEND;TZID=America/Chicago:20260127T180000\r\nRRULE:FREQ=WEEKLY;UNTIL=20260409T045959Z;BYDAY=TU\r\nEXDATE;TZID=America/Chicago:20260203T170000\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Choir moved\r\nRECURRENCE-ID;TZID=America/Chicago:20260217T170000\r\nDTSTART;TZID=America/Chicago:20260218T150000\r\nDURATION:PT90M\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn exdates_are_skipped_and_overrides_replace_their_instance() {
        assert_eq!(
            starts(WEEKLY, "2026-01-26T00:00:00Z", "2026-02-25T00:00:00Z"),
            vec![
                "01-27 23:00 60 Choir",
                "02-10 23:00 60 Choir",
                "02-18 21:00 90 Choir moved",
                "02-24 23:00 60 Choir",
            ]
        );
    }

    #[test]
    fn the_wall_clock_holds_across_daylight_saving() {
        let found = starts(WEEKLY, "2026-03-09T00:00:00Z", "2026-03-11T00:00:00Z");
        assert_eq!(found, vec!["03-10 22:00 60 Choir"]);
    }

    #[test]
    fn until_ends_the_series() {
        assert!(starts(WEEKLY, "2026-04-10T00:00:00Z", "2026-05-01T00:00:00Z").is_empty());
    }

    #[test]
    fn an_override_moved_into_the_range_appears_alone() {
        assert_eq!(
            starts(WEEKLY, "2026-02-18T00:00:00Z", "2026-02-19T00:00:00Z"),
            vec!["02-18 21:00 90 Choir moved"]
        );
    }

    #[test]
    fn yearly_all_day() {
        let data = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:b\r\nSUMMARY:Birthday\r\nDTSTART;VALUE=DATE:19800304\r\nDTEND;VALUE=DATE:19800305\r\nRRULE:FREQ=YEARLY\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        assert_eq!(
            starts(data, "2026-03-01T00:00:00Z", "2026-03-10T00:00:00Z"),
            vec!["2026-03-04..2026-03-05 Birthday"]
        );
    }

    #[test]
    fn a_date_until_includes_its_day() {
        let data = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:d\r\nSUMMARY:Camp\r\nDTSTART;VALUE=DATE:20250804\r\nDTEND;VALUE=DATE:20250805\r\nRRULE:FREQ=DAILY;UNTIL=20250806\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        assert_eq!(
            starts(data, "2025-08-01T00:00:00Z", "2025-08-10T00:00:00Z"),
            vec![
                "2025-08-04..2025-08-05 Camp",
                "2025-08-05..2025-08-06 Camp",
                "2025-08-06..2025-08-07 Camp"
            ]
        );
    }

    #[test]
    fn a_zero_length_moment() {
        let data = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:c\r\nSUMMARY:Moment\r\nDTSTART:20261104T162000Z\r\nDTEND:20261104T162000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        assert_eq!(
            starts(data, "2026-11-04T00:00:00Z", "2026-11-05T00:00:00Z"),
            vec!["11-04 16:20 0 Moment"]
        );
    }
}
