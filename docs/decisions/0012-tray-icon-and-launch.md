# 0012 — The tray icon next to the clock, and what a launch shows

**Status:** accepted, 2026-10-25. Amends decision 0004 (desktop integration and lifecycle).

## Context

The first real install showed two problems. Windows 11 puts every new tray icon among the hidden ones, behind the chevron, so the author did not find Volute's icon. And a launch from the Start menu, with no Volute running, started hidden behind that icon: nothing appeared, and Volute seemed not to open at all. The flyout also had a taskbar button, since M1.

## Decision

* **The icon goes next to the clock, once.** Explorer keeps a record per program and icon under `HKCU\Control Panel\NotifyIconSettings`; its `IsPromoted` value is what "Other system tray icons" in the taskbar settings writes. When that value is absent, no choice has been made: Volute writes 1. When it is present, whatever its value, it is the user's (or Volute's earlier) choice and is never touched again. Only Volute's own record is read or written (its executable path and icon ID).
* **Explorer reads the record when a process adds its icon**, and writes it when a program adds one for the first time. So the very first run records the choice for later runs, and every later run makes it before adding the icon. Removing and adding the icon again, even from a new window, does not make Explorer read it again (tried and measured), and Volute does not restart itself for it.
* **Test runs never promote their icon** (`Tray::new`'s `promote` is off for `--mock` and `--no-settings`).
* **A launch by hand always shows something:** the flyout, from the tray icon (or from the chevron when the icon is hidden), as a second launch already did. Only the start at sign-in waits behind the icon: the `Run` value carries `--startup`, from the installer and from Settings. A value written without the flag (M1) gets it when Volute starts.
* **The flyout has no taskbar button.** It is taken off once the window is on screen; asking before it appears was undone by its appearing.

## Consequences

* The record is undocumented. If Windows renames or drops it, Volute finds nothing and the icon stays where Windows puts it: the same as before, never worse.
* After a fresh install the icon is next to the clock from the second start of Volute (the next sign-in, or the next launch). The first launch opens the main window (installer) or the flyout (by hand), so Volute is never lost meanwhile.
