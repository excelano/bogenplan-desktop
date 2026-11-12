# CLAUDE.md

Bogenplan for the Linux desktop: the calendars in a CalDAV account (iCloud
first) laid out in Bogenzeit, an egui application. It is the desktop sibling of
excelano/bogenplan, the iPhone and iPad app, and follows that repository's
`docs/concept.md`; the Bogenzeit spec in excelano/bogenzeit is normative. The
only events it makes itself are moments, zero-length events with an alert at
their start; an opened `.ics` file is added as the file's own text, less
`METHOD`, `ORGANIZER` and `ATTENDEE`, one resource per UID, replacing a copy
with the same UID (`store/src/import.rs`). It edits and deletes events by
rewriting the fields it changes in the event's own iCalendar text
(`store/src/edit.rs`), never regenerating it, and writes with If-Match on the
cached etag. Subscribed calendars are read-only: those the account subscribes to
(`cs:subscribed` in the calendar home, fetched from their `cs:source`) and those
subscribed in Bogenplan by address, which need no account and are kept in the
keyring under `service=bogenplan-feed` with their color. `store/src/feed.rs`
fetches each without credentials and over HTTPS only, at most hourly or on Sync
Now, and the store refuses writes to them.

## Layout

The store crate, `store/` (`bogenplan-store`) is CalDAV, iCalendar and
recurrence behind `CalendarSource`: `libdav`, `icalendar`, `rrule` and the
keyring types stay inside it and the app sees calendars and occurrences only.
Its worker thread runs tokio, syncs on a timer, and publishes immutable
snapshots the window reads without waiting. The app crate is the window:
`day.rs` is the day view and the press-and-hold probe, `axis.rs` the 400-span
axis, `display.rs` the display forms, `glyph.rs` the drawn glyphs, `import.rs`
the sheet for an opened `.ics` file, `subscribe.rs` the subscription field and
the sheet for an opened `webcal://` link, `instance.rs` the session-bus name
`com.excelano.Bogenplan` through which a second launch hands its files and links
to the open window, `demo.rs` the sample week behind `--demo`.

The engine is the `bz` crate from excelano/bogenzeit, from crates.io.

## Commands

    cargo test --workspace
    cargo run -- --demo --date 2026-11-04
    cargo run --example sync -p bogenplan-store -- <cache dir>
    ./packaging/debian/build-deb.sh
    ./packaging/linux/check-libraries.sh

The `screenshot` feature lets a build photograph its own window:
`BOGENPLAN_SCREENSHOT=<png>` saves one after `BOGENPLAN_SCREENSHOT_DELAY`
seconds and closes, `BOGENPLAN_SCREENSHOT_SIZE=<w>x<h>` sizes the window, and
`BOGENPLAN_SCREENSHOT_OPEN` opens `settings`, `moment` or `event:<title>` first.
Point `XDG_CONFIG_HOME` and `XDG_CACHE_HOME` at a scratch directory to run
against a live account without touching the real profile.

## Rules

The day view gives every span the same height, from the starting ○ to the ending
○ (spec §3.8), and falls back to an even ○-to-○ axis on a polar day. Displayed
positions are the floor of the signed value. ☉, ○, △ and ▽ are drawn by
`glyph.rs`, never set in a font, so no glyph characters in UI strings. The
window is the night palette whatever the desktop theme, because the sky is the
content. Calendar data and the place stay on the machine; the only connections
are to the calendar server and to the publishers of subscribed calendars. Don't commit, push or publish unless David asks.
