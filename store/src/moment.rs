// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use chrono::{DateTime, Utc};

pub(crate) fn ics(uid: &str, at: DateTime<Utc>, title: &str, now: DateTime<Utc>) -> String {
    let stamp = |time: DateTime<Utc>| time.format("%Y%m%dT%H%M%SZ").to_string();
    let title = escape(title);
    [
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".to_string(),
        "PRODID:-//Excelano//Bogenplan//EN".to_string(),
        "CALSCALE:GREGORIAN".to_string(),
        "BEGIN:VEVENT".to_string(),
        format!("UID:{uid}"),
        format!("DTSTAMP:{}", stamp(now)),
        format!("CREATED:{}", stamp(now)),
        format!("DTSTART:{}", stamp(at)),
        format!("DTEND:{}", stamp(at)),
        format!("SUMMARY:{title}"),
        "TRANSP:TRANSPARENT".to_string(),
        "BEGIN:VALARM".to_string(),
        "ACTION:DISPLAY".to_string(),
        format!("DESCRIPTION:{title}"),
        "TRIGGER:PT0S".to_string(),
        "END:VALARM".to_string(),
        "END:VEVENT".to_string(),
        "END:VCALENDAR".to_string(),
        String::new(),
    ]
    .iter()
    .map(|line| fold(line))
    .collect::<Vec<_>>()
    .join("\r\n")
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

fn fold(line: &str) -> String {
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
    use crate::ics::{Stamp, parse};

    #[test]
    fn a_moment_reads_back_as_a_zero_length_event() {
        let at: DateTime<Utc> = "2026-11-04T16:20:00Z".parse().unwrap();
        let title = "59.2 spans before solar noon, with a comma; and more text to make the line fold past seventy-five octets";
        let text = ics("u1", at, title, at);
        assert!(text.lines().all(|line| line.len() <= 76));
        let events = parse(&text).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, title);
        assert_eq!(events[0].start.to_utc(), at);
        assert_eq!(events[0].end.as_ref().map(Stamp::to_utc), Some(at));
    }
}
