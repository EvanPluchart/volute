<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/png/volute-horizontal-reversed.png">
    <img src="assets/brand/png/volute-horizontal.png" alt="Volute" width="320">
  </picture>
</p>

<p align="center"><strong>Every app's volume, every device, one click from the clock.</strong><br>
A volume mixer for Windows 10 and 11. Native, light, free and open source.</p>

<p align="center">
  <a href="https://github.com/EvanPluchart/volute/releases/latest"><strong>Download</strong></a> ·
  <a href="https://evanpluchart.github.io/volute/en/">Website</a> ·
  <a href="https://evanpluchart.github.io/volute/">Site en français</a>
</p>

<p align="center">
  <img src="docs/screenshots/flyout-dark.png" alt="Volute's mixer opened from the clock: the master output, then one line per app with its icon, volume and live level, then the microphone" width="360">
</p>

## Features

Windows' own mixer is four clicks deep in Settings, shows no levels and forgets your apps. Volute sits next to the clock instead.

* **One line per app**, with its real name and icon, its volume, and what it is playing right now, glowing in amber inside the fader.
* **Mute in one click** on an app's icon.
* **Switch the output** from the top of the mixer.
* **Every device** in the main window: speakers, headsets and microphones, with their volume and level.
* **Remembers** the volume you give an app, and gives it back the next time it plays.
* **A speaker next to the clock** whose waves follow the volume. Click it for the mixer, scroll over it to change the volume.
* **Updates itself** in one click, after checking the update's signature.
* **Dark and light**, in English and French.

<p align="center">
  <img src="docs/screenshots/main-light.png" alt="The main window in the light theme: Mixer, Devices, Settings and About, and the mixer view" width="720">
</p>

## Download

Get the latest version from the [releases page](https://github.com/EvanPluchart/volute/releases/latest):

* **`Volute-x.y.z-setup.exe`** installs Volute for your account, without administrator rights, and can start it with Windows.
* **`Volute-x.y.z-portable.zip`** is a single executable to run from anywhere.

The executables are not code-signed yet, so Windows SmartScreen may warn on first launch: choose "More info", then "Run anyway". `SHA256SUMS` lets you check what you downloaded.

## Light by design

Volute stays open all day, so it costs nothing while it waits: **0 % CPU and about 4 MB of memory** in the background, about 3 % of one core while the mixer is open. It is written in Rust with [Slint](https://slint.dev): no browser engine, no runtime to install, a 6 MB installer.

## Privacy

* Volute reads the audio sessions and devices Windows reports, and changes a volume only when you ask.
* It never records the microphone; it only shows its level while another app is using it.
* Its only network request asks GitHub whether a new version is out. No account, no telemetry.
* It keeps its settings in `%APPDATA%\Volute` and needs no administrator rights.

## Build from source

Rust 1.88 or later, on Windows:

```sh
cargo run -p volute -- --open          # the app, with its window open
cargo run -p volute -- --mock --open   # on a demo instead of your real devices
cargo xtask check                      # build, tests and lints
cargo xtask dist                       # installer and portable zip (needs Inno Setup 6)
```

How it is built is in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), and what comes next in the [roadmap](docs/ROADMAP.md). To report a security problem, see [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE). Slint is used under its royalty-free license.
