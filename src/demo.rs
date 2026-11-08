// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use bogenplan_store::{Calendar, CalendarId, CalendarSource, Occurrence, OccurrenceTime, Reply};
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::America::Chicago;

use crate::place::Place;

pub const PLACE: Place = Place {
    latitude: 29.76,
    longitude: -95.37,
};

pub fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 4, 16, 20, 0)
        .single()
        .expect("the demo instant exists")
}

pub struct Demo;

struct Entry {
    weekdays: &'static [u32],
    start: Option<u32>,
    minutes: i64,
    title: &'static str,
    calendar: usize,
    location: Option<&'static str>,
    notes: Option<&'static str>,
}

const WORK: usize = 0;
const HOME: usize = 1;
const FAMILY: usize = 2;
const WEEKDAYS: &[u32] = &[2, 3, 4, 5, 6];

const fn entry(
    weekdays: &'static [u32],
    start: Option<u32>,
    minutes: i64,
    title: &'static str,
    calendar: usize,
) -> Entry {
    Entry {
        weekdays,
        start,
        minutes,
        title,
        calendar,
        location: None,
        notes: None,
    }
}

const fn at(
    weekdays: &'static [u32],
    hour: u32,
    minute: u32,
    minutes: i64,
    title: &'static str,
    calendar: usize,
) -> Entry {
    entry(weekdays, Some(hour * 60 + minute), minutes, title, calendar)
}

const fn located(mut entry: Entry, location: &'static str) -> Entry {
    entry.location = Some(location);
    entry
}

const WEEK: &[Entry] = &[
    entry(&[4], None, 0, "Ava's birthday", FAMILY),
    entry(&[5], None, 0, "Grandpa visiting", FAMILY),
    entry(&[5], None, 0, "Sam out of office", WORK),
    entry(&[5], None, 0, "Payday", HOME),
    entry(&[5], None, 0, "Recycling pickup", HOME),
    located(at(&[2, 4, 6], 6, 30, 45, "Run", HOME), "Buffalo Bayou"),
    at(WEEKDAYS, 9, 0, 15, "Stand-up", WORK),
    at(&[2], 13, 0, 60, "Client call", WORK),
    at(&[3], 11, 0, 45, "One-on-one with Sam", WORK),
    located(at(&[3], 19, 0, 120, "Book club", HOME), "Brazos Bookstore"),
    Entry {
        location: Some("Studio 3"),
        notes: Some("Walk through the new layout before it goes to the client."),
        ..at(&[4], 10, 30, 90, "Design review", WORK)
    },
    located(at(&[4], 12, 15, 60, "Lunch with Maya", HOME), "Common Bond"),
    located(at(&[4], 14, 0, 60, "Quarterly planning", WORK), "Room 4B"),
    at(&[4], 14, 30, 30, "Call with Lee", WORK),
    at(&[4], 16, 30, 45, "Dentist", HOME),
    located(at(&[4], 18, 0, 90, "Choir rehearsal", FAMILY), "St. Paul's"),
    at(&[5], 15, 0, 90, "Workshop", WORK),
    at(&[6], 18, 30, 120, "Dinner with the Riveras", FAMILY),
    at(&[7], 9, 0, 90, "Farmers market", FAMILY),
    at(&[7], 16, 0, 90, "Soccer game", FAMILY),
    at(&[1], 10, 30, 90, "Brunch", FAMILY),
];

fn calendar(index: usize, title: &str, color: [u8; 3]) -> Calendar {
    Calendar {
        id: CalendarId(format!("demo {index}")),
        title: title.to_string(),
        color,
        account: "Demo".to_string(),
        order: index as i32,
    }
}

fn day_events(day: NaiveDate) -> Vec<Occurrence> {
    let weekday = day.weekday().number_from_sunday();
    let calendars = Demo.calendars();
    WEEK.iter()
        .filter(|entry| entry.weekdays.contains(&weekday))
        .map(|entry| {
            let time = match entry.start {
                Some(minute) => {
                    let wall = day
                        .and_hms_opt(minute / 60, minute % 60, 0)
                        .expect("a demo time is a time of day");
                    let start = Chicago
                        .from_local_datetime(&wall)
                        .earliest()
                        .expect("demo times avoid the gap")
                        .with_timezone(&Utc);
                    OccurrenceTime::Timed {
                        start,
                        end: start + Duration::minutes(entry.minutes),
                    }
                }
                None => OccurrenceTime::AllDay {
                    start: day,
                    end: day + Duration::days(1),
                },
            };
            Occurrence {
                calendar: calendars[entry.calendar].id.clone(),
                key: format!("{day} {}", entry.title),
                title: entry.title.to_string(),
                location: entry.location.map(str::to_string),
                notes: entry.notes.map(str::to_string),
                time,
            }
        })
        .collect()
}

impl CalendarSource for Demo {
    fn calendars(&self) -> Vec<Calendar> {
        vec![
            calendar(WORK, "Work", [0xF0, 0xA0, 0x4B]),
            calendar(HOME, "Home", [0x6B, 0xC4, 0x90]),
            calendar(FAMILY, "Family", [0xD2, 0x7B, 0xD8]),
        ]
    }

    fn occurrences(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Vec<Occurrence> {
        let mut day = from.with_timezone(&Chicago).date_naive() - Duration::days(1);
        let last = to.with_timezone(&Chicago).date_naive();
        let mut found = Vec::new();
        while day <= last {
            found.extend(
                day_events(day)
                    .into_iter()
                    .filter(|occurrence| occurrence.time.overlaps(from, to)),
            );
            day += Duration::days(1);
        }
        found
    }

    fn add_moment(&self, _: DateTime<Utc>, _: String, _: &CalendarId) -> Reply {
        let (sender, receiver) = std::sync::mpsc::channel();
        let _ = sender.send(Err("The demo has no calendar to add to.".to_string()));
        receiver
    }
}
