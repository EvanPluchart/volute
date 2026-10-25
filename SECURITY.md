# Security

## Reporting a problem

Please report security problems privately, through GitHub's **Report a vulnerability** button on the repository's Security tab, rather than in a public issue. Say what you found, how to reproduce it, and which version you run (Settings, at the bottom). You will get an answer within a week.

## What Volute does, and what it never does

Volute runs with your rights only, never as administrator. It reads which apps play sound, their names and icons from their executable files, and the mouse wheel only while the pointer is over its tray icon. It never captures the microphone. It goes online only to check GitHub for a new version and to download it when you accept. The details are in the [README](README.md#what-it-reads-and-what-it-never-does) and in [decision 0005](docs/decisions/0005-distribution-updates-security.md).

## Updates

An update is installed only if its `SHA256SUMS` is signed with the release key built into Volute ([decision 0010](docs/decisions/0010-release-signing.md)) and the installer matches its checksum. Only a newer version is ever installed, and a Volute running as administrator never runs a downloaded file.

## Audits

* [2026-10-25](docs/audits/2026-10-25-security.md): the whole repository, with the updater. One medium and five low findings, all fixed.
