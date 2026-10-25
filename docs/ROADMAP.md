# Roadmap

Each milestone ends with its exit criteria met, checked by hand on Windows 11, `cargo xtask check` passing locally (decision 0006), and the budget measured. Work on the current milestone only.

## M0 — Skeleton

* Workspace, crates and their dependency rules, pinned Slint, `cargo xtask check`, `cargo xtask bench-overhead`.
* `volute-audio`: the `Audio` trait, the Windows backend listing render devices and the sessions of the default device with volume, mute and peak, and `MockAudio`.
* `volute-app`: a plain window listing those sessions, each with a slider, a mute button and a live meter; dragging the slider changes the app's volume.
* Lifecycle from day one: explicit quit, exit watchdog, single instance with acknowledgement, test flags.

**Exit:**
* `cargo run -p volute -- --open` lists the apps playing sound.
* A test changes the volume and mute of a session it started itself, and leaves the others alone.
* Closing the window ends the process, and a test proves it.
* The bench prints the open and idle cost.
* `cargo xtask check` passes.

## Design track (alongside M1 and M2)

* `docs/BRAND.md` turned into tokens in `ui/theme.slint`: colours (light and dark), type, spacing, radii, motion.
* Logo and app icon with the `logo-design` skill (brief in BRAND.md): concepts first, then the kit for the chosen one.
* Volute's own controls: slider with meter, mute toggle, switch, segmented control, menu, tooltip.
* A UX audit at the end of each milestone (working notes, kept out of the repository).

## M1 — The mixer

* One row per app: icon, name, volume slider with its live level, mute.
  * Processes grouped by executable.
  * System sounds kept apart.
  * Apps that stopped playing fade out after a while.
* Devices: master volume and mute of each output; the default output switched in one click; inputs with their level.
* Everything live from Core Audio events: no polling except meters while visible.
* Volumes remembered per app and applied when the app plays again.
* The mixer flyout from the tray, closing when it loses focus; scroll on the tray icon for the master volume.

**Exit:** usable as the only mixer for a day. Idle in the tray within budget.

## M2 — Keys and the OSD

* Volume keys swallowed and handled by Volute, with its own OSD:
  * device name, volume, mute state, live level;
  * dark and light, animated in and out;
  * on the monitor in use, never taking focus.
* A modifier with the volume keys sets the app in the foreground instead of the master.
* Global shortcuts: open the mixer at the pointer, mute the mic, cycle the output device.
* The hook is optional: off, Windows' own flyout comes back.

**Exit:** a week of daily use without a missed or doubled key press, no input lag, no OSD over a full-screen game unless allowed.

## M3 — Profiles and routing

* Profiles: named sets of app volumes, mutes and default devices, applied from the tray or a shortcut.
* Per-app output device (decision 0002: undocumented API, degrades gracefully).
* Ducking (optional): lower other apps while a chosen app plays, such as a call app.

## M4 — Distribution and updates

Pulled forward at the author's request on 2026-10-25, alongside M1's day of use: the installer, `cargo xtask dist`, the updater, Settings and About, the README and the landing page are built. What is left is publishing them: the repository made public, Pages turned on, and the first release.


* Inno Setup installer, per user, no administrator rights, start with Windows as an option.
* Updates from the app, as Vigie and Recto do (decision 0005): check at launch and every 6 hours, one click to install, checksum verified, relaunch.
* Release built locally with `cargo xtask dist`, publishing `SHA256SUMS` (decision 0006); landing page in French and English; About window with the Slint attribution.
* **The repository must be public before the first release with the updater:** for anyone outside it, a private repository's `releases/latest` answers nothing, so the check would find nothing.

## Later, maybe

Linux with PipeWire, an equalizer, per-app audio effects, a Stream Deck plugin, a small CLI (`volute set spotify 30`).
