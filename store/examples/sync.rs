// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use std::sync::mpsc;
use std::time::Duration;

use bogenplan_store::{CalendarSource, Status, Store};
use chrono::Utc;

fn main() {
    let cache = std::env::args()
        .nth(1)
        .expect("usage: sync <cache directory>");
    let (changed, wait) = mpsc::channel();
    let store = Store::open(cache.into(), move || {
        let _ = changed.send(());
    });
    loop {
        let _ = wait.recv_timeout(Duration::from_secs(1));
        match store.status() {
            Status::Syncing => continue,
            status => {
                println!("{status:?}");
                break;
            }
        }
    }
    for calendar in store.calendars() {
        println!(
            "{} ({}) {:?}",
            calendar.title, calendar.account, calendar.color
        );
    }
    let now = Utc::now();
    let week = store.occurrences(now, now + chrono::Duration::days(7));
    println!("{} occurrences in the next seven days", week.len());
}
