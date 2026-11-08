# Bogenplan for the Linux desktop

Bogenplan shows your calendars in Bogenzeit, time read as a position on the
Sun's arc. A day runs from one solar midnight to the next and holds 400 spans:
sunrise sits at 100, solar noon at 200 and sunset at 300, so daylight always
fills the middle half of the day and the events are what move with the seasons.
In Houston a 17:00 meeting sits mid-afternoon in June and just before sunset in
December. Every event also shows its civil time, because the people you meet
keep the clock.

This is the desktop sibling of the Bogenplan iPhone and iPad app. The iPhone
reads calendars through the device; a desktop has no shared calendar store that
every platform agrees on, so this one talks CalDAV to the server that holds your
calendars, which is iCloud for most iPhone users and works the same with
Fastmail, Nextcloud or any other CalDAV host.

## What it does

It lays out a day, several days or a week by the width of the window, with the
twilight bands drawn behind them and the present moment marked. Choosing an
event shows its start and end as positions on the arc and in civil time, its
length in spans and the eighth of the day it starts in.

It writes one kind of event: a moment. Press and hold an empty part of a day and
a line shows the reading at that point; keep still until it turns gold and let
go, or double-click, and a sheet sets the moment a step at a time. Adding it saves
a zero-length event with an alert at its start to the calendar you choose, so the
alert reaches every device that has that calendar. Bogenplan never edits or
deletes an event.

## Install

On Debian and Ubuntu, from the Excelano apt repository:

```
curl -fsSL https://excelano.com/apt/setup.sh | sudo sh && sudo apt install bogenplan
```

## Building

It is a Rust workspace and builds with a current stable toolchain:

    cargo build --release
    ./packaging/debian/build-deb.sh

The second line writes a `.deb` to `dist/` with the desktop entry, icon and
manual page. `cargo run --release -- --demo` opens a sample week without an
account.

## First run

Sign in with your server, user name and password. For iCloud the server is
`caldav.icloud.com`, the user name is your Apple Account, and the password is an
app-specific one made at account.apple.com under Sign-In and Security; your
Apple Account password is refused by the server and should not be used. The
password goes into the desktop keyring under `service=bogenplan`, so an entry
made with `secret-tool` works as well.

Solar noon, sunrise and sunset depend on where you are. Bogenplan asks GNOME's
Location Services through GeoClue for an approximate place, or you can enter
latitude and longitude in Settings.

## Privacy

Events are cached in `~/.cache/bogenplan`, readable only by you, and the place
and settings stay in `~/.config/bogenplan`. The only network connection is to
the calendar server you sign in to.

## The standard

Bogenzeit is defined in [excelano/bogenzeit](https://github.com/excelano/bogenzeit),
which also holds the `bz` engine this app computes with.

## License

MIT.
