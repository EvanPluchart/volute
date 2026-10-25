# 0004 — Desktop integration and lifecycle

**Status:** accepted, 2026-10-24. Amended by 0009 (closing) and 0012 (the icon next to the clock, what a launch shows).

## Context

Volute runs all day in the tray. Vigie, the author's task manager, shipped a version whose process stayed alive after its window closed. It kept its single instance, swallowed every later launch and grew in memory, and the user had no way back short of `taskkill`. Volute starts with the fixes that came out of that, and out of the audit that followed.

## Decision

### Lifecycle

* Every thread (audio, hook, tray) runs a loop that ends on an explicit `Quit`. No loop waits for a channel to close: callbacks hold senders.
* On quit, in this order:
  1. arm an exit watchdog (`process::exit` after 3 s);
  2. send `Quit` to every thread;
  3. release the single instance;
  4. hide the windows;
  5. remove the tray icon.
* Frames go to the UI with at most one in flight. A stalled UI never makes memory grow.
* A process with neither a visible window nor a tray icon quits. Closing to the tray needs the tray icon actually shown; adding it can fail while Explorer restarts.
* An integration test closes the window (`taskkill` without `/F`) with and without the tray, and checks the process ends.

### Single instance

* A named mutex, `Local\Volute.SingleInstance`.
* A second launch asks the running one to show its flyout, with `SendMessageTimeout`, and waits for an acknowledgement sent from the UI thread.
* With no answer within 2.5 s, the second launch starts anyway: two Volutes are better than none when the first hangs.

### Tray

* Win32 code ported from Vigie:
  * it ignores other programs' `WM_CLOSE`;
  * it quits on `WM_ENDSESSION`;
  * it adds itself back on `TaskbarCreated`;
  * it reports whether the icon is really shown.
* Left click opens the flyout at the icon, scroll changes the master volume, and right-click shows the menu (Open, Mute all, profiles, Settings, Quit).

### Settings

* Plain text in `%APPDATA%\Volute\settings.ini`, written atomically (a temporary file, then a rename).
* Every value is validated and unknown keys are kept. A file that cannot be parsed means defaults, never a failed start.
* What the user changes in the UI is saved; test flags never are.
* Start with Windows through `HKCU\…\Run`, set by the installer or Settings.
