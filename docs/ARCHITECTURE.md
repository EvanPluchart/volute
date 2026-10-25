# Architecture

Volute is a Cargo workspace with three library layers and one binary. Three rules shape everything:

1. **The UI never asks the OS for anything, and the audio layer never knows about the UI.**
2. **Volute lives in the tray all day: its idle cost is a feature.** Nothing polls while nothing is shown.
3. **A volume change feels instant.** From key press or slider drag to the change applied: under 16 ms.

```
crates/
  volute-audio   Windows Core Audio behind one trait: devices, sessions, volumes, peaks,      (no UI)
                 change notifications
  volute-core    the mixer model: sessions grouped by app, remembered volumes, profiles,      (no UI, no OS calls)
                 key bindings, settings
  volute-shell   desktop integration: tray, keyboard hook, OSD placement, single instance,    (no UI)
                 settings file, updates
  volute-app     Slint application, binary "volute"; ui/*.slint holds every view
xtask/           bench-overhead, dist
```

## Layers

### volute-audio: the sound system

```rust
pub trait Audio: Send {
    /// Every render device (and capture device) with its volume, mute and default role.
    fn devices(&mut self, out: &mut Vec<Device>) -> Result<()>;
    /// Every audio session of a device: process, display name, icon path, volume, mute, state.
    fn sessions(&mut self, device: &DeviceId, out: &mut Vec<Session>) -> Result<()>;
    /// Peak levels (0.0 to 1.0) of the given sessions and devices. Called only while a meter is shown.
    fn peaks(&mut self, ids: &[MeterId], out: &mut Vec<f32>) -> Result<()>;
    fn set_volume(&mut self, target: &Target, volume: f32) -> Result<()>;
    fn set_mute(&mut self, target: &Target, muted: bool) -> Result<()>;
    fn set_default_device(&mut self, device: &DeviceId, role: Role) -> Result<()>;
}
```

* **Windows backend (decision 0002):** Core Audio through the `windows` crate.
  * Devices: `IMMDeviceEnumerator`; volume and mute through `IAudioEndpointVolume`.
  * Sessions: `IAudioSessionManager2` → `IAudioSessionControl2`, giving the process, display name, icon path and whether it is the system-sounds session. Their volume and mute through `ISimpleAudioVolume`.
  * Peaks: `IAudioMeterInformation`, read on demand.
* **Events, not polling:**
  * `IMMNotificationClient`: devices added, removed or changed default.
  * `IAudioSessionNotification`: new sessions.
  * `IAudioSessionEvents`: volume, mute, state and disconnect, per session.
  * `IAudioEndpointVolumeCallback`: master volume.

  Callbacks arrive on COM threads. They only push a small event into a channel; they never touch the UI and never block.
* **Identity:**
  * A session is keyed by its session instance identifier.
  * An app is keyed by its executable path, not its PID: a browser has many processes, and a game restarts with a new PID.
  * Remembered volumes are per app.
* **The default output device** is set through `IPolicyConfig`, an undocumented interface every mixer uses, for all three roles (console, multimedia, communications) at once, as EarTrumpet does: one click moves every sound. **Per-app output devices** use `IAudioPolicyConfigFactory`, also undocumented, behind a feature that degrades gracefully. Decision 0002 records both.
* **A mock backend** (`MockAudio`) implements the trait for tests and screenshots: scripted devices, sessions and peaks. Tests never change the volume of a real app.

### volute-core: the model

* Groups sessions into apps (one row per app, its processes folded), sorts them (playing first, then by name), and keeps volumes the user set per app.
* **Converts between the linear scalar Windows stores and what a person hears.** Sliders and keys move in perceptual steps, so 50 % sounds half as loud.
* Holds profiles, key bindings and settings (parse and write, every value validated, unknown keys kept).
* Pure: everything here has unit tests without Windows.

### volute-shell: the desktop

* **Tray:** icon with tooltip, left click opens the mixer flyout at the tray, scroll changes the master volume, right-click menu.
* **Keyboard hook (decision 0003):** `WH_KEYBOARD_LL` on a thread of its own, looking only at the volume keys and Volute's own shortcuts. It swallows the volume keys so Windows' flyout stays hidden, then posts the event and returns at once.
* **OSD window placement:** which monitor, where (above the taskbar, centred, or as chosen), work area; the window itself is Slint.
* **Single instance** (named mutex, and a message to the running one with an acknowledgement), settings file (atomic write), start with Windows (HKCU Run), updates (decision 0005).

### volute-app: the interface

* Slint, every view in `ui/*.slint`, every token in `ui/theme.slint` (from `docs/BRAND.md`). Volute draws its own controls (sliders, meters, switches, menus): native widgets do not match the art direction.
* **Windows:**
  * the **mixer flyout**, compact, opened from the tray or a shortcut, closing when it loses focus;
  * the **main window**: a rail with Mixer, Devices, Settings and About (Profiles come in M3);
  * the **OSD**, a borderless, topmost window that never takes focus.
* The audio thread sends frames with `slint::invoke_from_event_loop`, at most one in flight. Models are patched (`set_row_data`, inserts, removals), never replaced.

## Threads

| Thread | Owns | Does |
|---|---|---|
| UI (main) | Slint windows | Renders, patches models, sends commands |
| audio | the `Audio` backend (COM, MTA) | Applies commands, turns COM events into model updates, reads peaks while a meter shows |
| hook | the `WH_KEYBOARD_LL` hook and its message loop | Filters volume keys and shortcuts, posts them, returns immediately |
| tray | the tray window and its message loop | Tray clicks, scroll, menu, notifications |
| update | nothing but its timer | One HTTPS request at start (3 s) and every 6 hours; parks in between (decision 0005). Never joined: it holds nothing |
| download | the update's files while they download | Fetches, verifies the signature and the checksum, hands the held installer to the UI thread, ends |
| COM callbacks | Windows' | Push events into the audio thread's channel; nothing else |

* Commands go UI → audio over a `flume` channel. Events and frames go back through `invoke_from_event_loop`.
* **Lifecycle (decision 0004):**
  * Quitting sends an explicit `Quit` to each thread.
  * An exit watchdog (3 s) is armed before anything else is torn down.
  * The single instance is released early.
  * No thread waits for a channel to close.

## Performance budget

| State | CPU (one core) | Private memory |
|---|---|---|
| Idle in the tray, nothing shown | < 0.1 % | < 25 MB |
| Mixer open, meters running | < 4 % (target 2 %, see below) | < 40 MB |
| OSD showing | < 1 % for its 1.5 s | — |

* Meters read peaks at 30 Hz only while a meter is visible. Nothing ticks when nothing is shown.
* From a key press to the volume being applied: under 16 ms. The OSD appears in the next frame.
* **The open state is allowed 4 % for now.** With Slint 1.18 on winit and the software renderer, every meter update renders twice, whatever the window holds (measured in M0: 2.6 to 3.4 %). After the redesign (2026-10-25), on the mock, the worst case where every row moves every frame: 3.0 % with the main window open, 2.95 % with the flyout, 0.000 % and 3.5 MB idle (`docs/audits/2026-10-25-performance.md`).
* **A window must not grow on screen.** Slint 1.18 redraws only what changed, so a part of a window that was off screen, or uncovered by a resize the renderer did not see, stays blank. The main window is sized before it shows; a flyout whose rows change its height is opened again at the new height.
* `cargo xtask bench-overhead` measures the idle and open states, run by hand on an idle machine; a release checks it.

## Conventions

* `cargo clippy -- -D warnings` clean; `unsafe` only in `volute-audio` and `volute-shell`, each block with a `// SAFETY:` comment.
* Settings and profiles are plain text in `%APPDATA%\Volute`, written atomically.
* Test flags reach any state without clicking. `--no-settings` touches no file and takes no single instance. `--no-hook` installs no keyboard hook. `--no-update-check` goes offline. `--mock` runs on `MockAudio`, so no real app is touched. Tests use them all.
* Errors reach the user as a short message in the UI, never a silent failure. Logs go to stderr; nothing is written to disk except settings.
