<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/png/volute-horizontal-reversed.png">
    <img src="assets/brand/png/volute-horizontal.png" alt="Volute" width="320">
  </picture>
</p>

<p align="center"><strong>Every app's volume, every device, one click from the tray.</strong><br>
A volume mixer for Windows 10 and 11, native, light, and pleasant to touch.</p>

<p align="center">
  <img src="docs/screenshots/flyout-dark.png" alt="Volute's mixer opened from the tray: the master output, then one line per app with its icon, volume and live level, then the microphone" width="360">
</p>

> **Status: early.** The mixer works and 0.1.0 is the first release. The volume keys and their on-screen display come next ([roadmap](docs/ROADMAP.md)).

## What it does

Windows' own mixer is four clicks deep in Settings, shows no levels and forgets your apps. Volute sits in the notification area instead:

* **One line per app**, with its real name and icon, its volume, and its live level glowing in amber inside the fader. Processes of the same app are one line; system sounds stay apart.
* **Mute where you look:** a click on an app's icon mutes it.
* **Switch the output** from the top of the mixer, in one click: Volute sets it as Windows' default for every role.
* **Every device** in the main window: each output's volume and mute, the microphones with their level.
* **Remembers:** the volume you give an app in Volute comes back the next time it plays.
* **From the tray:** a speaker next to the clock, like Windows' own, whose amber waves follow the master volume; a click opens the mixer, the wheel over it moves the master volume, the menu mutes everything. Closing the main window keeps Volute there (a setting).
* **Updates itself:** it looks for a new version at start and every 6 hours, and installs it in one click after checking its checksum. You can turn that off.
* **Dark and light**, following Windows or set by hand, in English and French.

<p align="center">
  <img src="docs/screenshots/main-light.png" alt="The main window in the light theme: a rail with Mixer, Devices, Settings and About, and the mixer view" width="720">
</p>

## What it costs

Volute stays in the tray all day, so it is built to cost nothing while it waits. Measured with `cargo xtask bench-overhead` on the author's machine:

| State | CPU (one core) | Memory |
|---|---|---|
| Waiting in the tray | 0.000 % | about 4 MB |
| Mixer open, meters running | 2 to 4 % | about 6 MB |
| Applying a volume | 0.12 ms | |

It is Rust and [Slint](https://slint.dev) with a software renderer: no browser engine, no runtime to install: a 6.4 MB installer, one executable. Volume changes and device switches come from Windows' own audio events; nothing polls except the meters, and only while you look at them.

## What it reads, and what it never does

* **Sound:** the audio sessions and devices Windows reports, their volumes and levels. Volute changes a volume only when you ask, or to give an app back the volume you set for it.
* **Apps:** each sound's executable path, to show the name and icon stored in the file. It opens no other process's memory.
* **The mouse wheel over its tray icon, and nothing else** ([decision 0007](docs/decisions/0007-tray-wheel.md)): only the wheel, only while the pointer is over the icon, kept nowhere.
* **The microphone:** never captured. Its level shows while another app uses it ([decision 0008](docs/decisions/0008-microphone-level.md)).
* **Network:** one request to GitHub to see if a new version is out, and the download when you accept it. No account, no telemetry, no crash upload.
* **Files:** `settings.ini` and `volumes.ini` in `%APPDATA%\Volute`, and a downloaded update in `%LOCALAPPDATA%\Volute\Updates` until it is installed.
* **Registry:** the `Volute` value under `Run` when you choose to start with Windows, and, once, Windows' own "show next to the clock" choice for Volute's icon, never against a choice you made ([decision 0012](docs/decisions/0012-tray-icon-and-launch.md)). Nothing else is written.
* **Rights:** none beyond yours. It installs for your account only and never asks for administrator rights.

## Install

Download it from the [releases page](https://github.com/EvanPluchart/volute/releases/latest):

* **`Volute-x.y.z-setup.exe`**: installs for your account in `%LOCALAPPDATA%\Programs\Volute`, no administrator prompt, optional start with Windows. Updates itself.
* **`Volute-x.y.z-portable.zip`**: one executable to run from anywhere. It tells you when a new version is out.
* **`SHA256SUMS`** and **`SHA256SUMS.sig`**: the checksums of both, signed with the release key built into Volute; the updater checks both before running anything ([decision 0010](docs/decisions/0010-release-signing.md)).

The binaries are not signed yet, so Windows SmartScreen may warn on first run.

## Build from source

Rust 1.88 or later, on Windows:

```sh
cargo run -p volute -- --open          # the app, with its window open
cargo run -p volute -- --mock --open   # on a scripted demo instead of your real devices
cargo xtask check                      # check, test, clippy: what CI would run (decision 0006)
cargo xtask bench-overhead             # CPU and memory, open and idle
cargo xtask dist                       # the installer, the zip, SHA256SUMS and its signature, into dist/
```

`cargo xtask dist` needs [Inno Setup 6](https://jrsoftware.org/isinfo.php) (`winget install JRSoftware.InnoSetup`).

## Documentation

* [Architecture](docs/ARCHITECTURE.md): layers, threads, the audio model, the performance budget.
* [Roadmap](docs/ROADMAP.md): milestones and their exit criteria.
* [Brand](docs/BRAND.md) and the [logo kit](assets/brand/README.md).
* [Decisions](docs/decisions/): settled choices, one file each.
* [Security](SECURITY.md): how to report a problem, and the audit.
* [CLAUDE.md](CLAUDE.md): the rules for agents working in this repository.

## License

[MIT](LICENSE). Slint is used under its royalty-free licence, credited in the About view. The landing page's typeface, Figtree, is under the SIL Open Font License.
