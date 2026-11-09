# Security Policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately through GitHub Security
Advisories at https://github.com/excelano/bogenplan-desktop/security/advisories/new.
If you would rather not use GitHub, email david.anderson@excelano.com instead. I
aim to respond within seven days.

Please do not open public issues for security problems.

## Supported versions

The latest 0.x release receives security fixes. Older versions are not
supported.

## What this application is

Bogenplan reads the calendars in a CalDAV account, adds zero-length events to
one of them, and edits or deletes events the user chooses. It connects to the calendar server the user signs in to over
HTTPS, with the system's trusted certificates, and to nothing else: no
telemetry, no update check, no account of its own. The place is typed in by the
user and kept on this computer.

Calendar data arrives from the server as iCalendar text, parsed by the
`icalendar` and `rrule` crates and by the code in `store/src`. A malformed or
hostile event is the input this application has to withstand, and a crash or
hang on one is worth reporting.

## What it stores

The password, in the desktop keyring through the Secret Service, never on disk
by Bogenplan itself. The events last fetched, in
`~/.cache/bogenplan/calendars.json`, created readable by its owner only. The
settings and the place, in `~/.config/bogenplan/settings.json`.

## Verifying releases

Every release lists a SHA-256 for each artefact. Verify a download before
running it.
