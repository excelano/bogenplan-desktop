// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CalendarId(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Calendar {
    pub id: CalendarId,
    pub title: String,
    pub color: [u8; 3],
    pub account: String,
    pub order: i32,
    #[serde(default)]
    pub read_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OccurrenceTime {
    AllDay {
        start: NaiveDate,
        end: NaiveDate,
    },
    Timed {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
}

impl OccurrenceTime {
    pub fn overlaps(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> bool {
        let (start, end) = self.utc_bounds();
        start < to && (end > from || (start == end && start >= from))
    }

    pub fn start_instant(&self) -> DateTime<Utc> {
        match *self {
            OccurrenceTime::Timed { start, .. } => start,
            OccurrenceTime::AllDay { start, .. } => start
                .and_hms_opt(0, 0, 0)
                .expect("midnight exists")
                .and_utc(),
        }
    }

    pub fn utc_bounds(&self) -> (DateTime<Utc>, DateTime<Utc>) {
        match *self {
            OccurrenceTime::Timed { start, end } => (start, end),
            OccurrenceTime::AllDay { start, end } => (
                start
                    .and_hms_opt(0, 0, 0)
                    .expect("midnight exists")
                    .and_utc()
                    - chrono::Duration::hours(14),
                end.and_hms_opt(0, 0, 0).expect("midnight exists").and_utc()
                    + chrono::Duration::hours(12),
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Occurrence {
    pub calendar: CalendarId,
    pub key: String,
    pub href: String,
    pub uid: String,
    pub original: Option<DateTime<Utc>>,
    pub title: String,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub time: OccurrenceTime,
}

impl Occurrence {
    pub fn repeats(&self) -> bool {
        self.original.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    One,
    All,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub title: String,
    pub location: String,
    pub notes: String,
    pub time: OccurrenceTime,
    pub calendar: CalendarId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub server: String,
    pub username: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Syncing,
    Synced(DateTime<Utc>),
    NoAccounts,
    Failed(String),
}
