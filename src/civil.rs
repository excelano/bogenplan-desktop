// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Utc, Weekday};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clock {
    TwelveHour,
    TwentyFourHour,
}

impl Clock {
    pub fn time(self, at: DateTime<Utc>) -> String {
        let local = at.with_timezone(&Local);
        match self {
            Clock::TwelveHour => local.format("%-I:%M %p").to_string(),
            Clock::TwentyFourHour => local.format("%H:%M").to_string(),
        }
    }

    pub fn date_time(self, at: DateTime<Utc>) -> String {
        format!(
            "{}, {}",
            at.with_timezone(&Local).format("%a %-d %b %Y"),
            self.time(at)
        )
    }
}

pub fn date_of(day: i64) -> bz::Date {
    bz::Date::from_days_since_epoch(day)
}

pub fn naive(date: bz::Date) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year, date.month, date.day)
        .expect("a Bogenzeit date is a calendar date")
}

pub fn day_of(date: NaiveDate) -> i64 {
    bz::Date {
        year: date.year(),
        month: date.month(),
        day: date.day(),
    }
    .days_since_epoch()
}

pub fn local_midnight(date: NaiveDate) -> DateTime<Utc> {
    let midnight = date.and_hms_opt(0, 0, 0).expect("midnight exists");
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .unwrap_or_else(|| Local.from_utc_datetime(&midnight))
        .with_timezone(&Utc)
}

pub fn civil_interval(date: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
    (
        local_midnight(date),
        local_midnight(date + chrono::Duration::days(1)),
    )
}

pub fn first_weekday() -> Weekday {
    let locale = ["LC_TIME", "LC_ALL", "LANG"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
        .unwrap_or_default();
    let region = locale
        .split(['.', '@'])
        .next()
        .unwrap_or("")
        .rsplit('_')
        .next()
        .unwrap_or("");
    const SUNDAY_REGIONS: &[&str] = &[
        "US", "CA", "MX", "BR", "JP", "KR", "TW", "HK", "IL", "PH", "IN", "ZA", "AU", "SA", "PE",
        "CO", "VE", "GT",
    ];
    if SUNDAY_REGIONS.contains(&region) {
        Weekday::Sun
    } else {
        Weekday::Mon
    }
}

pub fn week_start(date: NaiveDate, first: Weekday) -> NaiveDate {
    let back = (date.weekday().num_days_from_sunday() + 7 - first.num_days_from_sunday()) % 7;
    date - chrono::Duration::days(i64::from(back))
}
