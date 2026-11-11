// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use crate::edit;
use crate::expand;
use crate::ics;
use crate::model::OccurrenceTime;

#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    pub events: Vec<Imported>,
    pub invitation: bool,
    pub cancellation: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    pub uid: String,
    pub title: String,
    pub time: OccurrenceTime,
    pub repeats: bool,
    pub(crate) text: String,
}

impl Import {
    pub fn read(data: &str) -> Result<Import, String> {
        ics::parse(data).map_err(|error| format!("This is not a calendar file ({error})."))?;
        let copies = edit::own_copies(data);
        let events: Vec<Imported> = copies
            .events
            .into_iter()
            .filter_map(|(uid, text)| {
                let parsed = ics::parse(&text).ok()?;
                let master = parsed
                    .iter()
                    .find(|event| event.recurrence_id.is_none())
                    .or(parsed.first())?;
                Some(Imported {
                    uid,
                    title: master.summary.clone(),
                    time: expand::single(master),
                    repeats: master.rrule.is_some() || !master.rdates.is_empty(),
                    text,
                })
            })
            .collect();
        if events.is_empty() {
            return Err("There is no event in this file.".to_string());
        }
        let method = copies.method.unwrap_or_default();
        Ok(Import {
            events,
            invitation: method == "REQUEST",
            cancellation: method == "CANCEL",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INVITATION: &str = "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nPRODID:Microsoft Exchange Server 2010\r\nVERSION:2.0\r\nBEGIN:VTIMEZONE\r\nTZID:Central Standard Time\r\nBEGIN:STANDARD\r\nDTSTART:16010101T020000\r\nTZOFFSETFROM:-0500\r\nTZOFFSETTO:-0600\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\nBEGIN:VEVENT\r\nORGANIZER;CN=Ana Lopez:mailto:ana@example.com\r\nATTENDEE;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION;RSVP=TRUE;CN=Some\r\n one:mailto:someone@example.com\r\nDESCRIPTION:Agenda\\, attached\r\nUID:040000008200E00074C5B7101A82E008\r\nSUMMARY:Quarterly review\r\nDTSTART;TZID=Central Standard Time:20261116T110000\r\nDTEND;TZID=Central Standard Time:20261116T120000\r\nBEGIN:VALARM\r\nACTION:DISPLAY\r\nTRIGGER:-PT15M\r\nEND:VALARM\r\nEND:VEVENT\r\nBEGIN:VTODO\r\nUID:todo\r\nSUMMARY:Not an event\r\nEND:VTODO\r\nEND:VCALENDAR\r\n";

    #[test]
    fn an_invitation_becomes_a_copy_without_scheduling() {
        let import = Import::read(INVITATION).unwrap();
        assert!(import.invitation);
        assert!(!import.cancellation);
        assert_eq!(import.events.len(), 1);
        let event = &import.events[0];
        assert_eq!(event.uid, "040000008200E00074C5B7101A82E008");
        assert_eq!(event.title, "Quarterly review");
        assert!(!event.repeats);
        let text = &event.text;
        for gone in ["METHOD", "ORGANIZER", "ATTENDEE", "VTODO"] {
            assert!(!text.contains(gone), "{gone} is still in {text}");
        }
        for kept in [
            "BEGIN:VTIMEZONE",
            "TZOFFSETTO:-0600",
            "DESCRIPTION:Agenda\\, attached",
            "TRIGGER:-PT15M",
            "PRODID:Microsoft Exchange Server 2010",
        ] {
            assert!(text.contains(kept), "{kept} is missing from {text}");
        }
        let start: chrono::DateTime<chrono::Utc> = "2026-11-16T17:00:00Z".parse().unwrap();
        assert_eq!(
            event.time,
            OccurrenceTime::Timed {
                start,
                end: start + chrono::Duration::hours(1)
            }
        );
    }

    #[test]
    fn each_uid_is_its_own_copy_and_a_missing_uid_is_made() {
        let data = "BEGIN:VCALENDAR\nVERSION:2.0\nBEGIN:VEVENT\nUID:a\nSUMMARY:Series\nDTSTART:20261102T150000Z\nRRULE:FREQ=WEEKLY\nEND:VEVENT\nBEGIN:VEVENT\nSUMMARY:No uid\nDTSTART;VALUE=DATE:20261105\nEND:VEVENT\nBEGIN:VEVENT\nUID:a\nRECURRENCE-ID:20261109T150000Z\nSUMMARY:Moved\nDTSTART:20261109T170000Z\nEND:VEVENT\nEND:VCALENDAR\n";
        let import = Import::read(data).unwrap();
        assert!(!import.invitation);
        assert_eq!(import.events.len(), 2);
        let series = &import.events[0];
        assert!(series.repeats);
        assert_eq!(series.title, "Series");
        assert_eq!(ics::parse(&series.text).unwrap().len(), 2);
        let made = &import.events[1];
        assert!(!made.uid.is_empty());
        assert!(made.text.contains(&format!("UID:{}", made.uid)));
        assert!(!made.text.contains("UID:a"));
        assert!(matches!(made.time, OccurrenceTime::AllDay { .. }));
    }

    #[test]
    fn a_cancellation_and_a_file_without_events() {
        let cancel = INVITATION.replace("METHOD:REQUEST", "METHOD:CANCEL");
        assert!(Import::read(&cancel).unwrap().cancellation);
        let empty = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nEND:VCALENDAR\r\n";
        assert!(Import::read(empty).is_err());
        assert!(Import::read("not a calendar").is_err());
    }
}
