# 0007 — The mouse wheel on the tray icon

**Status:** accepted, 2026-10-24

## Context

M1 asks for the master volume to follow the mouse wheel over Volute's tray icon, as EarTrumpet and the system's own volume icon allow. Windows does not send wheel messages to tray icons: `Shell_NotifyIcon` reports clicks and hovering, never the wheel. Reading it means reading mouse input that is not addressed to Volute's windows, which this repository treats as carefully as the keyboard hook (decision 0003).

## Decision

* **Raw mouse input, only while the pointer is over the icon**, as EarTrumpet does (File-New-Project/EarTrumpet, `ShellNotifyIcon.cs` and `InputHelper.cs`):
  1. The icon reports the pointer moving over it (`WM_MOUSEMOVE` through its callback message).
  2. Volute asks where the icon is (`Shell_NotifyIconGetRect`). If the pointer (`GetCursorPos`) is inside, it registers its hidden tray window for raw mouse input (`RegisterRawInputDevices`, generic page, mouse usage, `RIDEV_INPUTSINK`).
  3. On each `WM_INPUT`, it reads only the wheel flag and its delta, and only acts if the pointer is still inside the icon's rectangle.
  4. The first `WM_INPUT` with the pointer outside unregisters (`RIDEV_REMOVE`). So does removing the icon.
* **Rules it never breaks:**
  * **Wheel only.** Movement, buttons and every other field are ignored. Nothing is stored, logged or sent anywhere; the delta becomes volume steps and is dropped.
  * **Only over the icon.** No raw input is registered while the pointer is elsewhere, and none at all with `--no-tray`.
  * **Cheap.** The handler reads one structure and posts one event to the UI thread. It never blocks.
  * **The keyboard is never involved.** The keyboard hook stays decision 0003's, and comes in M2.
* **Steps:** one notch (a delta of 120) moves the master by 2 %. Smaller deltas from precision touchpads add up until they make a notch.

## Consequences

* Moving the pointer quickly across the icon registers and unregisters raw input once each; it costs nothing measurable.
* If Explorer restarts, the icon is added again (decision 0004) and the rectangle is read again on the next hover.
* The README states what the tray reads, next to what the keyboard hook reads (M2).
