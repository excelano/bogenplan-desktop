// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use chrono::{DateTime, Duration, NaiveDate, Utc};
use icalendar::parser::unfold;
use rrule::Tz;

use crate::ics::{self, Event, Stamp, escape, fold};
use crate::model::{Change, OccurrenceTime, Scope};

#[derive(Debug, PartialEq)]
pub(crate) enum Write {
    Put(String),
    Delete,
}

pub(crate) struct Target<'a> {
    pub uid: &'a str,
    pub original: Option<DateTime<Utc>>,
    pub time: OccurrenceTime,
}

pub(crate) fn delete(
    data: &str,
    target: &Target,
    scope: Scope,
    now: DateTime<Utc>,
) -> Result<Write, String> {
    let mut document = Document::read(data);
    let series = match target.original {
        Some(original) if scope == Scope::One => original,
        _ => {
            document.remove_events(target.uid);
            return Ok(document.written());
        }
    };
    document.remove_override(target.uid, series);
    if let Some(index) = document.master(target.uid) {
        let form = document.start_form(index)?;
        let block = document.block(index);
        block.push(form.line("EXDATE", series));
        block.touch(now);
    }
    Ok(document.written())
}

pub(crate) fn change(
    data: &str,
    target: &Target,
    scope: Scope,
    change: &Change,
    now: DateTime<Utc>,
) -> Result<String, String> {
    let mut document = Document::read(data);
    let (index, created) = match target.original {
        None => (
            document
                .single(target.uid)
                .ok_or("The event is no longer in its calendar.")?,
            false,
        ),
        Some(_) if scope == Scope::All => {
            let index = document
                .master(target.uid)
                .ok_or("The repeating event this belongs to is not in its calendar.")?;
            document.shift_series(index, target, change)?;
            document.set_text(index, change)?;
            document.block(index).touch(now);
            return Ok(document.text());
        }
        Some(original) => match document.override_of(target.uid, original) {
            Some(index) => (index, false),
            None => (document.add_override(target.uid, original)?, true),
        },
    };
    if change.time != target.time || created {
        let form = document.start_form(index)?;
        document.block(index).set_time(&form, change.time);
    }
    document.set_text(index, change)?;
    document.block(index).touch(now);
    Ok(document.text())
}

pub(crate) struct Copies {
    pub method: Option<String>,
    pub events: Vec<(String, String)>,
}

const OUTSIDE_EVENTS: [&str; 4] = ["VCALENDAR", "VTIMEZONE", "STANDARD", "DAYLIGHT"];
const SCHEDULING: [&str; 2] = ["ORGANIZER", "ATTENDEE"];

pub(crate) fn own_copies(data: &str) -> Copies {
    let document = Document::read(data);
    let mut method = None;
    let mut components: Vec<String> = Vec::new();
    let mut header = Vec::new();
    for (index, part) in document.parts.iter().enumerate() {
        let Part::Line(line) = part else {
            continue;
        };
        let upper = line.to_ascii_uppercase();
        if let Some(component) = upper.strip_prefix("BEGIN:") {
            components.push(component.trim().to_string());
        }
        let kept = components
            .iter()
            .all(|component| OUTSIDE_EVENTS.contains(&component.as_str()));
        let is_method = components.len() == 1 && name_of(line).eq_ignore_ascii_case("METHOD");
        if is_method {
            method = Some(split(line).1.trim().to_ascii_uppercase());
        }
        if kept && !is_method {
            header.push(index);
        }
        if upper.starts_with("END:") {
            components.pop();
        }
    }
    let mut groups: Vec<(String, Vec<(usize, Block)>)> = Vec::new();
    for (index, block) in document.events() {
        let mut block = block.clone();
        let uid = match block.value("UID").map(|uid| uid.trim().to_string()) {
            Some(uid) if !uid.is_empty() => uid,
            _ => {
                let uid = uuid::Uuid::new_v4().to_string().to_uppercase();
                block.set("UID", format!("UID:{uid}"));
                uid
            }
        };
        for name in SCHEDULING {
            block.remove(name);
        }
        match groups.iter_mut().find(|(found, _)| *found == uid) {
            Some((_, blocks)) => blocks.push((index, block)),
            None => groups.push((uid, vec![(index, block)])),
        }
    }
    let events = groups
        .into_iter()
        .map(|(uid, blocks)| {
            let mut parts = Vec::new();
            for (index, part) in document.parts.iter().enumerate() {
                if let Part::Line(line) = part
                    && header.contains(&index)
                {
                    parts.push(Part::Line(line.clone()));
                }
                if let Some((_, block)) = blocks.iter().find(|(at, _)| *at == index) {
                    parts.push(Part::Event(block.clone()));
                }
            }
            (uid, Document { parts }.text())
        })
        .collect();
    Copies { method, events }
}

enum Part {
    Line(String),
    Event(Block),
}

struct Document {
    parts: Vec<Part>,
}

impl Document {
    fn read(data: &str) -> Document {
        let unfolded = unfold(data);
        let mut parts = Vec::new();
        let mut current: Option<Vec<String>> = None;
        let mut depth = 0usize;
        for line in unfolded.lines().map(|line| line.trim_end_matches('\r')) {
            if line.is_empty() {
                continue;
            }
            let upper = line.to_ascii_uppercase();
            match &mut current {
                Some(lines) => {
                    lines.push(line.to_string());
                    if upper.starts_with("BEGIN:") {
                        depth += 1;
                    } else if upper.starts_with("END:") {
                        if depth == 0 {
                            parts.push(Part::Event(Block {
                                lines: current.take().expect("inside an event"),
                            }));
                        } else {
                            depth -= 1;
                        }
                    }
                }
                None if upper == "BEGIN:VEVENT" => {
                    current = Some(vec![line.to_string()]);
                    depth = 0;
                }
                None => parts.push(Part::Line(line.to_string())),
            }
        }
        Document { parts }
    }

    fn text(&self) -> String {
        let mut lines = Vec::new();
        for part in &self.parts {
            match part {
                Part::Line(line) => lines.push(fold(line)),
                Part::Event(block) => lines.extend(block.lines.iter().map(|line| fold(line))),
            }
        }
        lines.push(String::new());
        lines.join("\r\n")
    }

    fn written(&self) -> Write {
        if self.events().next().is_none() {
            Write::Delete
        } else {
            Write::Put(self.text())
        }
    }

    fn events(&self) -> impl Iterator<Item = (usize, &Block)> {
        self.parts
            .iter()
            .enumerate()
            .filter_map(|(index, part)| match part {
                Part::Event(block) => Some((index, block)),
                Part::Line(_) => None,
            })
    }

    fn block(&mut self, index: usize) -> &mut Block {
        match &mut self.parts[index] {
            Part::Event(block) => block,
            Part::Line(_) => unreachable!("an index from events() is an event"),
        }
    }

    fn parsed(&self, index: usize) -> Option<Event> {
        let Part::Event(block) = &self.parts[index] else {
            return None;
        };
        let mut lines: Vec<&str> = Vec::new();
        let mut in_zone = false;
        for part in &self.parts {
            if let Part::Line(line) = part {
                let upper = line.to_ascii_uppercase();
                if upper == "BEGIN:VCALENDAR" || upper == "END:VCALENDAR" {
                    continue;
                }
                if upper == "BEGIN:VTIMEZONE" {
                    in_zone = true;
                }
                if in_zone {
                    lines.push(line);
                }
                if upper == "END:VTIMEZONE" {
                    in_zone = false;
                }
            }
        }
        let mut text = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n");
        for line in lines
            .into_iter()
            .chain(block.lines.iter().map(String::as_str))
        {
            text.push_str(line);
            text.push_str("\r\n");
        }
        text.push_str("END:VCALENDAR\r\n");
        ics::parse(&text).ok()?.into_iter().next()
    }

    fn of_uid(&self, uid: &str) -> Vec<usize> {
        self.events()
            .filter(|(_, block)| block.value("UID").as_deref() == Some(uid))
            .map(|(index, _)| index)
            .collect()
    }

    fn master(&self, uid: &str) -> Option<usize> {
        self.of_uid(uid)
            .into_iter()
            .find(|&index| self.prop_of(index, "RECURRENCE-ID").is_none())
    }

    fn single(&self, uid: &str) -> Option<usize> {
        self.master(uid)
            .or_else(|| self.of_uid(uid).into_iter().next())
    }

    fn prop_of(&self, index: usize, name: &str) -> Option<String> {
        match &self.parts[index] {
            Part::Event(block) => block.prop(name),
            Part::Line(_) => None,
        }
    }

    fn override_of(&self, uid: &str, original: DateTime<Utc>) -> Option<usize> {
        self.of_uid(uid).into_iter().find(|&index| {
            self.parsed(index)
                .and_then(|event| event.recurrence_id)
                .is_some_and(|stamp| stamp.to_utc() == original)
        })
    }

    fn remove_events(&mut self, uid: &str) {
        let doomed = self.of_uid(uid);
        self.remove(&doomed);
    }

    fn remove_override(&mut self, uid: &str, original: DateTime<Utc>) {
        if let Some(index) = self.override_of(uid, original) {
            self.remove(&[index]);
        }
    }

    fn remove(&mut self, indices: &[usize]) {
        let mut index = 0;
        self.parts.retain(|_| {
            let keep = !indices.contains(&index);
            index += 1;
            keep
        });
    }

    fn start_form(&self, index: usize) -> Result<Form, String> {
        let line = self
            .prop_of(index, "DTSTART")
            .ok_or("The event has no start time.")?;
        let event = self.parsed(index).ok_or("The event could not be read.")?;
        Ok(Form::of(&line, &event.start))
    }

    fn add_override(&mut self, uid: &str, original: DateTime<Utc>) -> Result<usize, String> {
        let master = self
            .master(uid)
            .ok_or("The repeating event this belongs to is not in its calendar.")?;
        let form = self.start_form(master)?;
        let mut block = match &self.parts[master] {
            Part::Event(block) => block.clone(),
            Part::Line(_) => unreachable!("master() returns an event"),
        };
        for name in ["RRULE", "RDATE", "EXDATE", "EXRULE"] {
            block.remove(name);
        }
        block.push(form.line("RECURRENCE-ID", original));
        self.parts.insert(master + 1, Part::Event(block));
        Ok(master + 1)
    }

    fn set_text(&mut self, index: usize, change: &Change) -> Result<(), String> {
        let event = self.parsed(index).ok_or("The event could not be read.")?;
        let block = self.block(index);
        let title = change.title.trim();
        if title != event.summary {
            block.set("SUMMARY", format!("SUMMARY:{}", escape(title)));
        }
        let location = change.location.trim();
        if location != event.location.as_deref().unwrap_or("") {
            block.remove("X-APPLE-STRUCTURED-LOCATION");
            block.set_or_remove("LOCATION", location);
        }
        let notes = change.notes.trim();
        if notes != event.description.as_deref().unwrap_or("") {
            block.set_or_remove("DESCRIPTION", notes);
        }
        Ok(())
    }

    fn shift_series(
        &mut self,
        master: usize,
        target: &Target,
        change: &Change,
    ) -> Result<(), String> {
        if change.time == target.time {
            return Ok(());
        }
        let form = self.start_form(master)?;
        let (delta, length) = match (target.time, change.time) {
            (
                OccurrenceTime::Timed { start, .. },
                OccurrenceTime::Timed {
                    start: new_start,
                    end: new_end,
                },
            ) => {
                if form.local_date(start) != form.local_date(new_start) {
                    return Err(MOVE_ONE.to_string());
                }
                (new_start - start, new_end - new_start)
            }
            (
                OccurrenceTime::AllDay { start, .. },
                OccurrenceTime::AllDay {
                    start: new_start,
                    end: new_end,
                },
            ) => {
                if start != new_start {
                    return Err(MOVE_ONE.to_string());
                }
                (Duration::zero(), new_end - new_start)
            }
            _ => {
                return Err(
                    "A whole series cannot switch between all-day and timed. Change this occurrence only."
                        .to_string(),
                );
            }
        };
        let event = self.parsed(master).ok_or("The event could not be read.")?;
        let start = event.start.to_utc() + delta;
        let block = self.block(master);
        block.remove("DURATION");
        block.set("DTSTART", form.line("DTSTART", start));
        block.set("DTEND", form.line("DTEND", start + length));
        if delta.is_zero() {
            return Ok(());
        }
        block.remove("EXDATE");
        for exdate in &event.exdates {
            block.push(form.line("EXDATE", exdate.to_utc() + delta));
        }
        let uid = target.uid.to_string();
        for index in self.of_uid(&uid) {
            if index == master {
                continue;
            }
            if let Some(original) = self.parsed(index).and_then(|event| event.recurrence_id) {
                let moved = form.line("RECURRENCE-ID", original.to_utc() + delta);
                self.block(index).set("RECURRENCE-ID", moved);
            }
        }
        Ok(())
    }
}

const MOVE_ONE: &str =
    "A repeating event keeps its days. To move it to another day, change this occurrence only.";

#[derive(Clone)]
struct Block {
    lines: Vec<String>,
}

impl Block {
    fn top_level(&self) -> impl Iterator<Item = (usize, &String)> {
        let mut depth = 0usize;
        let last = self.lines.len().saturating_sub(1);
        self.lines.iter().enumerate().filter(move |(index, line)| {
            if *index == 0 || *index == last {
                return false;
            }
            let upper = line.to_ascii_uppercase();
            if upper.starts_with("BEGIN:") {
                depth += 1;
                return false;
            }
            if upper.starts_with("END:") {
                depth = depth.saturating_sub(1);
                return false;
            }
            depth == 0
        })
    }

    fn prop(&self, name: &str) -> Option<String> {
        self.top_level()
            .find(|(_, line)| name_of(line).eq_ignore_ascii_case(name))
            .map(|(_, line)| line.clone())
    }

    fn value(&self, name: &str) -> Option<String> {
        self.prop(name).map(|line| split(&line).1.to_string())
    }

    fn remove(&mut self, name: &str) {
        let doomed: Vec<usize> = self
            .top_level()
            .filter(|(_, line)| name_of(line).eq_ignore_ascii_case(name))
            .map(|(index, _)| index)
            .collect();
        for index in doomed.into_iter().rev() {
            self.lines.remove(index);
        }
    }

    fn push(&mut self, line: String) {
        let at = self
            .lines
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, line)| {
                let upper = line.to_ascii_uppercase();
                upper.starts_with("BEGIN:") || upper.starts_with("END:")
            })
            .map_or(self.lines.len(), |(index, _)| index);
        self.lines.insert(at, line);
    }

    fn set(&mut self, name: &str, line: String) {
        self.remove(name);
        self.push(line);
    }

    fn set_or_remove(&mut self, name: &str, text: &str) {
        if text.is_empty() {
            self.remove(name);
        } else {
            self.set(name, format!("{name}:{}", escape(text)));
        }
    }

    fn set_time(&mut self, form: &Form, time: OccurrenceTime) {
        self.remove("DURATION");
        match time {
            OccurrenceTime::AllDay { start, end } => {
                self.set("DTSTART", date_line("DTSTART", start));
                self.set("DTEND", date_line("DTEND", end));
            }
            OccurrenceTime::Timed { start, end } => {
                let form = match form {
                    Form::Date => &Form::Utc,
                    other => other,
                };
                self.set("DTSTART", form.line("DTSTART", start));
                self.set("DTEND", form.line("DTEND", end));
            }
        }
    }

    fn touch(&mut self, now: DateTime<Utc>) {
        let stamp = now.format("%Y%m%dT%H%M%SZ");
        let sequence = self
            .value("SEQUENCE")
            .and_then(|value| value.trim().parse::<u32>().ok())
            .map_or(0, |sequence| sequence + 1);
        self.set("SEQUENCE", format!("SEQUENCE:{sequence}"));
        self.set("DTSTAMP", format!("DTSTAMP:{stamp}"));
        self.set("LAST-MODIFIED", format!("LAST-MODIFIED:{stamp}"));
    }
}

enum Form {
    Date,
    Zone(String, Tz),
    Utc,
    Floating,
}

impl Form {
    fn of(line: &str, stamp: &Stamp) -> Form {
        match stamp {
            Stamp::Date(_) => Form::Date,
            Stamp::Zoned(_, zone) => match param(line, "TZID") {
                Some(tzid) => Form::Zone(tzid, *zone),
                None if split(line).1.trim_end().to_ascii_uppercase().ends_with('Z') => Form::Utc,
                None => Form::Floating,
            },
        }
    }

    fn line(&self, name: &str, at: DateTime<Utc>) -> String {
        let local = |zone: Tz| at.with_timezone(&zone).format("%Y%m%dT%H%M%S").to_string();
        match self {
            Form::Date => date_line(name, at.date_naive()),
            Form::Zone(tzid, zone) => format!("{name};TZID={tzid}:{}", local(*zone)),
            Form::Utc => format!("{name}:{}", at.format("%Y%m%dT%H%M%SZ")),
            Form::Floating => format!("{name}:{}", local(Tz::LOCAL)),
        }
    }

    fn local_date(&self, at: DateTime<Utc>) -> NaiveDate {
        match self {
            Form::Zone(_, zone) => at.with_timezone(zone).date_naive(),
            Form::Floating => at.with_timezone(&Tz::LOCAL).date_naive(),
            Form::Date | Form::Utc => at.date_naive(),
        }
    }
}

fn date_line(name: &str, date: NaiveDate) -> String {
    format!("{name};VALUE=DATE:{}", date.format("%Y%m%d"))
}

fn split(line: &str) -> (&str, &str) {
    let mut quoted = false;
    for (index, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ':' if !quoted => return (&line[..index], &line[index + 1..]),
            _ => {}
        }
    }
    (line, "")
}

fn name_of(line: &str) -> &str {
    let head = split(line).0;
    head.split(';').next().unwrap_or(head)
}

fn param(line: &str, key: &str) -> Option<String> {
    split(line)
        .0
        .split(';')
        .skip(1)
        .filter_map(|param| param.split_once('='))
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, value)| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expand::instances;
    use crate::model::CalendarId;

    const SERIES: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VTIMEZONE\r\nTZID:America/Chicago\r\nBEGIN:STANDARD\r\nTZOFFSETFROM:-0500\r\nTZOFFSETTO:-0600\r\nDTSTART:19701101T020000\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\nBEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Choir\r\nDTSTART;TZID=America/Chicago:20260127T170000\r\nDTEND;TZID=America/Chicago:20260127T180000\r\nRRULE:FREQ=WEEKLY;UNTIL=20260409T045959Z;BYDAY=TU\r\nEXDATE;TZID=America/Chicago:20260203T170000\r\nATTENDEE;CN=Sam:mailto:sam@example.com\r\nBEGIN:VALARM\r\nACTION:DISPLAY\r\nDESCRIPTION:Choir\r\nTRIGGER:-PT15M\r\nEND:VALARM\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:a\r\nSUMMARY:Choir moved\r\nRECURRENCE-ID;TZID=America/Chicago:20260217T170000\r\nDTSTART;TZID=America/Chicago:20260218T150000\r\nDURATION:PT90M\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    const SINGLE: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:b\r\nSUMMARY:Lunch\r\nLOCATION:Common Bond\r\nX-APPLE-STRUCTURED-LOCATION;VALUE=URI;X-TITLE=Common Bond:geo:29.7,-95.4\r\nDTSTART:20261104T181500Z\r\nDTEND:20261104T191500Z\r\nSEQUENCE:2\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    fn utc(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn timed(start: &str, end: &str) -> OccurrenceTime {
        OccurrenceTime::Timed {
            start: utc(start),
            end: utc(end),
        }
    }

    fn change_to(title: &str, time: OccurrenceTime) -> Change {
        Change {
            title: title.to_string(),
            location: String::new(),
            notes: String::new(),
            time,
            calendar: CalendarId(String::new()),
        }
    }

    fn listed(data: &str, from: &str, to: &str) -> Vec<String> {
        let events = ics::parse(data).unwrap();
        let mut found: Vec<String> = instances(&events, utc(from), utc(to))
            .iter()
            .map(|instance| {
                let (start, end) = instance.time.utc_bounds();
                format!(
                    "{} {} {}",
                    start.format("%m-%d %H:%M"),
                    (end - start).num_minutes(),
                    instance.event.summary
                )
            })
            .collect();
        found.sort();
        found
    }

    fn now() -> DateTime<Utc> {
        utc("2026-02-01T12:00:00Z")
    }

    #[test]
    fn a_single_event_keeps_what_it_does_not_change() {
        let target = Target {
            uid: "b",
            original: None,
            time: timed("2026-11-04T18:15:00Z", "2026-11-04T19:15:00Z"),
        };
        let mut edit = change_to("Lunch with Maya", target.time);
        edit.location = "Common Bond".to_string();
        let text = change(SINGLE, &target, Scope::One, &edit, now()).unwrap();
        assert!(text.contains("SUMMARY:Lunch with Maya"));
        assert!(text.contains("X-APPLE-STRUCTURED-LOCATION"));
        assert!(text.contains("DTSTART:20261104T181500Z"));
        assert!(text.contains("SEQUENCE:3"));
        edit.location = "Brazos".to_string();
        edit.time = timed("2026-11-04T19:00:00Z", "2026-11-04T19:30:00Z");
        let text = change(SINGLE, &target, Scope::One, &edit, now()).unwrap();
        assert!(!text.contains("X-APPLE-STRUCTURED-LOCATION"));
        assert_eq!(
            listed(&text, "2026-11-04T00:00:00Z", "2026-11-05T00:00:00Z"),
            vec!["11-04 19:00 30 Lunch with Maya"]
        );
    }

    #[test]
    fn deleting_one_occurrence_skips_it_and_drops_its_override() {
        let target = Target {
            uid: "a",
            original: Some(utc("2026-02-10T23:00:00Z")),
            time: timed("2026-02-10T23:00:00Z", "2026-02-11T00:00:00Z"),
        };
        let Write::Put(text) = delete(SERIES, &target, Scope::One, now()).unwrap() else {
            panic!("expected a put");
        };
        assert!(text.contains("EXDATE;TZID=America/Chicago:20260210T170000"));
        assert!(text.contains("ATTENDEE;CN=Sam:mailto:sam@example.com"));
        assert!(text.contains("TRIGGER:-PT15M"));
        assert_eq!(
            listed(&text, "2026-01-26T00:00:00Z", "2026-02-25T00:00:00Z"),
            vec![
                "01-27 23:00 60 Choir",
                "02-18 21:00 90 Choir moved",
                "02-24 23:00 60 Choir",
            ]
        );
        let moved = Target {
            uid: "a",
            original: Some(utc("2026-02-17T23:00:00Z")),
            time: timed("2026-02-18T21:00:00Z", "2026-02-18T22:30:00Z"),
        };
        let Write::Put(text) = delete(SERIES, &moved, Scope::One, now()).unwrap() else {
            panic!("expected a put");
        };
        assert!(!text.contains("Choir moved"));
        assert!(text.contains("EXDATE;TZID=America/Chicago:20260217T170000"));
    }

    #[test]
    fn deleting_all_removes_the_resource() {
        let target = Target {
            uid: "a",
            original: Some(utc("2026-02-10T23:00:00Z")),
            time: timed("2026-02-10T23:00:00Z", "2026-02-11T00:00:00Z"),
        };
        assert_eq!(
            delete(SERIES, &target, Scope::All, now()).unwrap(),
            Write::Delete
        );
    }

    #[test]
    fn changing_one_occurrence_adds_an_override() {
        let target = Target {
            uid: "a",
            original: Some(utc("2026-02-10T23:00:00Z")),
            time: timed("2026-02-10T23:00:00Z", "2026-02-11T00:00:00Z"),
        };
        let edit = change_to("Choir, early", target.time);
        let text = change(SERIES, &target, Scope::One, &edit, now()).unwrap();
        assert!(text.contains("RECURRENCE-ID;TZID=America/Chicago:20260210T170000"));
        assert_eq!(
            listed(&text, "2026-02-09T00:00:00Z", "2026-02-12T00:00:00Z"),
            vec!["02-10 23:00 60 Choir, early"]
        );
        assert_eq!(
            listed(&text, "2026-02-23T00:00:00Z", "2026-02-25T00:00:00Z"),
            vec!["02-24 23:00 60 Choir"]
        );
    }

    #[test]
    fn changing_all_moves_the_time_of_day_and_keeps_skips_and_overrides() {
        let target = Target {
            uid: "a",
            original: Some(utc("2026-02-10T23:00:00Z")),
            time: timed("2026-02-10T23:00:00Z", "2026-02-11T00:00:00Z"),
        };
        let edit = change_to(
            "Choir",
            timed("2026-02-10T22:30:00Z", "2026-02-11T00:00:00Z"),
        );
        let text = change(SERIES, &target, Scope::All, &edit, now()).unwrap();
        assert_eq!(
            listed(&text, "2026-01-26T00:00:00Z", "2026-02-25T00:00:00Z"),
            vec![
                "01-27 22:30 90 Choir",
                "02-10 22:30 90 Choir",
                "02-18 21:00 90 Choir moved",
                "02-24 22:30 90 Choir",
            ]
        );
        let elsewhere = change_to(
            "Choir",
            timed("2026-02-11T23:00:00Z", "2026-02-12T00:00:00Z"),
        );
        assert_eq!(
            change(SERIES, &target, Scope::All, &elsewhere, now()),
            Err(MOVE_ONE.to_string())
        );
    }
}
