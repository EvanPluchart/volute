# 0009 — Closing the main window

**Status:** accepted, 2026-10-25. Amends decision 0004 (lifecycle).

## Context

Volute lives in the tray: the flyout is how it is used day to day, and the main window (mixer, devices, settings) is opened from time to time. Closing that window quit Volute (0004), which surprises anyone who expects a tray app to stay. Vigie, the author's task manager, faced the same question and answered with a setting, off by default.

## Decision

* **A setting, "Keep running when the window closes", off by default**, as in Vigie. Off, closing the main window quits, as before.
* **On, closing hides the window, only while the tray icon is really shown** (`Tray::is_shown`). Without an icon on screen, closing quits: a process with no window and no icon is still a bug (0004).
* **"Quit" in the tray menu always quits**, as does signing out.
* The flyout is not affected: it closes when it loses focus, and never quits Volute.

## Consequences

* 0004's integration test keeps its meaning: with the setting off (the default, and every test run's), closing the window ends the process, with or without the tray.
* Test runs (`--no-settings`, `--mock`) can turn the setting on for the session, and never save it.
