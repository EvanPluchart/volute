# 0003 — Volume keys and the on-screen display

**Status:** accepted, 2026-10-24

## Context

Windows shows a small grey flyout when the volume keys are pressed. It cannot be restyled, it shows nothing per app, and there is no supported switch to hide it. The author wants Volute's own display instead.

## Decision

* **A low-level keyboard hook (`WH_KEYBOARD_LL`)** on a thread of its own in `volute-shell`, with its own message loop.
  * It reads only `vkCode`. `VK_VOLUME_UP`, `VK_VOLUME_DOWN` and `VK_VOLUME_MUTE`, plus Volute's configured shortcuts, are swallowed: the hook returns 1, so Windows neither changes the volume nor shows its flyout. The event is posted to Volute, which applies the change and shows its OSD.
  * Every other key passes through untouched.
* **Rules the hook never breaks:**
  * **Privacy:** no other key is read beyond its code, stored, logged or sent anywhere. Code review checks this on every change to the hook.
  * **Speed:** Windows drops a hook that is too slow, and every keystroke on the machine waits for it. The callback does a comparison and a `PostThreadMessage`, nothing else: no allocation, no lock, no I/O.
  * **Elevated windows:** Volute never asks for more rights to see their keys. Whether the volume keys still reach the hook when an elevated window has focus is to be checked in M2. If they do not, Windows' flyout shows for that window, which is acceptable.
  * **Off means off:** with the hook disabled in Settings (or `--no-hook`), the keys reach Windows and its flyout returns.
  * **When Volute quits or crashes,** the hook goes with the process. Windows removes it, and the keys work as before.
* **The OSD:**
  * a Slint window, borderless, topmost, without a taskbar button, never activated, so it never steals focus from a game or a text field;
  * shown on the monitor with the foreground window, for about 1.5 s after the last key press;
  * it animates in and out, and follows the theme;
  * it shows the device's name, the volume, the mute state and a live level; with the app modifier, the app's icon and name.
* **Full-screen apps:** by default, the OSD does not show over an exclusive or borderless full-screen window (games); a setting allows it.
* **Changes that bypass the keyboard:**
  * headset buttons, OEM utilities and other apps never reach the hook;
  * Volute sees them through the endpoint volume callback;
  * M2 decides whether the OSD shows for them too. A sensible default: only when the change did not come from Volute itself.

## Consequences

* Antivirus programs look at keyboard hooks. A signed binary, a clear Settings switch, and this record are the answer; the README says what the hook does and does not do.
* The hook thread is the one place where latency is a correctness issue: a test measures the callback's time on a synthetic event.
