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

The one kind of event it makes itself is a moment. Press and hold an empty part of a day and
a line shows the reading at that point; keep still until it turns gold and let
go, or double-click, and a sheet sets the moment a step at a time or from a typed
reading, a number before or after ○, △, ☉ or ▽. Adding it saves
a zero-length event with an alert at its start to the calendar you choose, so the
alert reaches every device that has that calendar.

Any event can be edited or deleted from its window: the title, location, notes,
start and end, all-day or not, and the calendar it is in. Start and end are set
in Bogenzeit, with the same steps and typed readings as a moment, or in civil
time when civil time is shown. For a repeating event
Bogenplan asks whether the change is for that occurrence or the whole series; a
whole series keeps its days, so moving one to another day is done to that
occurrence. An edit rewrites only the fields it changes, so invitees, alarms and
repeat rules stay as they were, and a change made elsewhere since the last sync
is refused rather than overwritten.

It opens iCalendar files, so it can be the desktop's calendar application and
take the invitations a mail client hands it. The file's events are shown on the
arc and added to the calendar you choose. An invitation is saved as your own
copy, without its organizer and attendees, so the server sends nothing on your
behalf and Bogenplan does not reply. An event already in your calendars with
the same UID is replaced, which is how an updated invitation lands, and a
cancellation removes it. A file opened while Bogenplan is running goes to the
window already open.

Calendars the account subscribes to, such as a school's or a team's feed added
in iCloud, appear beside the others with their names and colours. Their events
can be read but not edited, and a moment cannot be added to one. Bogenplan
fetches each feed from its publisher at most once an hour, or when Sync Now is
pressed, and always over HTTPS.

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

Solar noon, sunrise and sunset depend on where you are, so Bogenplan asks for
your latitude and longitude on first run; Settings changes them later.

## Privacy

Events are cached in `~/.cache/bogenplan`, readable only by you, and the place
and settings stay in `~/.config/bogenplan`. Bogenplan connects to the calendar
server you sign in to and, for each calendar the account subscribes to, to the
server that publishes it. A publisher learns your address and when the feed was
fetched, and nothing else.

## The standard

Bogenzeit is defined in [excelano/bogenzeit](https://github.com/excelano/bogenzeit),
which also holds the `bz` engine this app computes with.

## License

MIT.
