# Volute

A volume mixer for Windows that is fast, pleasant to look at, and does what the built-in one does not: every app's volume and live level in one place, quick control from the keyboard and the tray, an on-screen display worth looking at when you press the volume keys, and switching outputs without digging through Settings.

Native Rust with [Slint](https://slint.dev). Light: it sits in the tray all day, so it must cost nothing.

> **Status:** M0 (skeleton) in progress. See [docs/ROADMAP.md](docs/ROADMAP.md). Windows first; Linux (PipeWire) may follow; macOS has no per-app volume without a driver and is out of scope.

## Why

Windows' mixer is buried in Settings, shows no levels, forgets apps, and its volume pop-up is a grey bar in a corner. Volute is:

* **One look:** every app playing sound, with its icon, volume, mute and a live level meter.
* **One gesture:**
  * scroll on the tray icon or press the volume keys for the master volume;
  * add a modifier to set the app in the foreground;
  * a global shortcut opens the mixer at the pointer.
* **A real OSD:** the volume keys show Volute's own display:
  * readable, animated, theme-aware;
  * the device's name;
  * the app's volume when an app is targeted.
* **Devices:** switch the default output in one click, per-device volumes, mics with their input level.
* **Remembers:** per-app volumes and mutes persist across restarts, and profiles hold a whole setup.

## Documentation

* [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): layers, threads, audio model, performance budget.
* [docs/ROADMAP.md](docs/ROADMAP.md): milestones and exit criteria.
* [docs/BRAND.md](docs/BRAND.md): art direction, design tokens, logo brief.
* [docs/decisions/](docs/decisions/): settled choices.
* [docs/PROMPT.md](docs/PROMPT.md): the brief for milestone M0.
* [CLAUDE.md](CLAUDE.md): rules for agents working in this repository.

## License

MIT. Slint is used under its royalty-free license (attribution in the About window).
