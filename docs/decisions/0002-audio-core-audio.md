# 0002 — Audio: Windows Core Audio, events first

**Status:** accepted, 2026-10-24

## Context

A per-app mixer needs the audio sessions of every app, their volume and mute, live peak levels, and notice when anything changes, without polling while nothing is shown.

## Decision

* **Core Audio through the `windows` crate**, in `volute-audio` only, on one audio thread (COM, multithreaded apartment).
  * Devices: `IMMDeviceEnumerator`, `IMMDevice`, `IAudioEndpointVolume`.
  * Sessions: `IAudioSessionManager2`, `IAudioSessionEnumerator`, `IAudioSessionControl2`, `ISimpleAudioVolume`.
  * Peaks: `IAudioMeterInformation`.
* **Events drive the model:**
  * `IMMNotificationClient`: devices.
  * `IAudioSessionNotification`: new sessions.
  * `IAudioSessionEvents`: changes per session.
  * `IAudioEndpointVolumeCallback`: master volume.

  The callbacks only send events into the audio thread's channel. Peaks are the one thing read on demand, at 30 Hz while a meter is visible.
* **Volume curve:** Windows stores a linear scalar. `volute-core` maps sliders and key steps through a perceptual curve, so equal steps sound equal, and keeps the stored value exact. Changes Volute makes carry its own event context GUID, so its own echo is not mistaken for another program's change.
* **Undocumented interfaces, isolated and optional:**
  * `IPolicyConfig` sets the default device. Every third-party mixer uses it, and it has been stable since Windows 7.
  * `IAudioPolicyConfigFactory` sets an app's output device (Windows 10 1803 and later). Its interface identifier changed between Windows versions.

  Both sit behind small functions that report "unsupported" instead of failing, are tested on the author's Windows 11, and their absence hides the feature rather than breaking the app.
* **Identity:**
  * Sessions are keyed by session instance identifier.
  * Apps are keyed by executable path, and system sounds are their own entry.
  * Remembered volumes are per app.
* **Never touch what the user did not ask to change.** Tests use `MockAudio`, or a session the test creates by playing a short sound itself.

## Consequences

* Windows only. Linux would be a second backend on PipeWire behind the same trait; macOS has no per-app volume without installing a driver, and is out of scope.
* COM objects never cross threads: the UI holds plain data.
