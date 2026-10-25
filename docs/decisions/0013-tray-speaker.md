# 0013 — The tray icon is a speaker

**Status:** accepted, 2026-10-25. Amends `docs/BRAND.md` (tray variants).

## Context

Next to the clock, the logo's violet tile read as one more app among the tray icons, not as the volume. The author asked for a speaker like Windows' own, in Volute's colours.

## Decision

* **A speaker drawn as Windows draws its own, with amber waves.** The body is white on a dark taskbar and ink (`#1B1530`) on a light one, like the system's icons; the waves are amber, Volute's one accent. Three directions were compared (this one, a violet speaker, the speaker on the app's tile); this one sits best among the system icons, and the amber still says which speaker is Volute's.
* **It shows the master volume:** no wave at 0 %, one up to 33 %, two up to 66 %, three above, and a red cross when muted, as Windows does.
* **It follows the taskbar's theme** (`SystemUsesLightTheme`), read at start and again on `WM_SETTINGCHANGE`.
* The ten icons are generated (`assets/brand/src/tray.mjs`, then `tray.sh`) and embedded as resources 10 to 14 (white) and 20 to 24 (ink). The app icon stays the logo's tile (resource 1), also used for notifications.

## Consequences

* The old tray icons (`icons/tray*.ico`, the V on its tile) are no longer in the executable.
* Changing the icon with the volume costs one `NIM_MODIFY` when the number of waves changes, not at every step.
