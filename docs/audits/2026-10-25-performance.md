# Performance audit — 2026-10-25

Scope: CPU, memory, latency and size after the redesign, the Settings and About views and the updater. Measured on the author's machine with `cargo xtask bench-overhead --seconds=60` (release build, 5 s warm-up, one core = 100 %).

## Measurements

| State | M1 (7274c45), mock | Now, mock | Budget |
|---|---|---|---|
| Main window open, meters running | 3.9 % CPU, 5.1 MB | **3.0 % CPU, 6.0 MB** | < 4 %, < 40 MB |
| Flyout open over the tray | 3.3 % CPU, 5.2 MB | **2.95 % CPU, 5.0 MB** | < 4 %, < 40 MB |
| Idle in the tray | 0.000 % CPU, 3.4 MB | **0.000 % CPU, 3.5 MB** | < 0.1 %, < 25 MB |

The mock is the worst case: every app, the master and the microphone move every frame. The real backend measured 1.8 % open in M1.

**The open state varies a lot from run to run on this machine.** After the keyboard and security commits, two runs back to back gave 4.85 % then 1.90 % open (flyout 3.38 % then 2.78 %; idle 0.000 % both times). The first is over the 4 % budget. Nothing changed between them, so other programs on the machine are the likely cause, but one run cannot tell. Run the bench three times on an idle machine before the release, and look again if the median is over 3 %.

* **Latency:** `set_volume` on a real session, 0.12 ms median (the owned-session test fails above 16 ms); the flyout is created and shown in about 1 ms.
* **Size:** `volute.exe` 10.9 MB, the installer 6.4 MB, the portable zip 5.7 MB.

## What was checked

* **Nothing polls at rest.** Volumes, mutes, devices and sessions come from Core Audio's events; the audio thread sleeps on its channel. Peaks are read at 30 Hz only while a mixer is on screen, and the main window stops them when minimised or covered (`Occluded`). Idle stays at 0.000 %.
* **The updater costs nothing between checks:** its thread parks for 6 hours; the check is one HTTPS request without a body. Nothing is loaded for it until then (WinHTTP and BCrypt are part of Windows).
* **One frame in flight.** A stalled UI never queues frames; memory stays flat over a run (peak and end within 0.2 MB).
* **Models are patched, never replaced**, so a level change redraws the level, not the list; the master and microphone levels redraw nothing for changes under 0.4 %.
* **The flyout is created on open and dropped on close**, so its frame buffer is freed while Volute waits.
* **The redesign is cheaper, not dearer:** flat shapes, no shadows, no transparency except the update banner's tint, no animation. The level is drawn inside the fill, one rectangle per fader.
* **App icons** are read once per executable at 48 px and cached; the demo's are drawn once.

## Findings

| # | Finding | Status |
|---|---|---|
| 1 | Slint 1.18's software renderer on winit renders each meter update twice; this is most of the open cost. | Known since M0, outside Volute; the budget allows 4 %. Revisit when Slint is upgraded (a dedicated commit, on request). |
| 2 | A window that grows on screen, or whose off-screen part comes back, is not drawn there (the renderer redraws only what changed). A flyout that gained a row while open showed a blank strip, in M1 already. | Fixed: the main window is sized before it shows, and a flyout whose height must change is opened again (about 1 ms). |
| 3 | The binary carries Slint's clipboard, menu and URL-opening crates that Volute never calls (they come with the winit backend). | Kept: they cost disk, not memory, since untouched pages are never loaded; trimming them means Slint feature work. |
| 4 | Release profile already: fat LTO, one codegen unit, stripped, `panic = abort`. | Nothing to do. |

## How to repeat it

`cargo xtask bench-overhead` (60 s per state by default, `--real` for this machine's devices, read only), on an idle machine. Update this table and the one in `docs/ARCHITECTURE.md`.

## Before 0.1.0: three runs

After the speaker icon, the foreground watch and the repaint fix, three runs back to back (60 s per state, mock):

| State | Run 1 | Run 2 | Run 3 | Budget |
|---|---|---|---|---|
| Main window open | 2.89 % | 3.01 % | 3.41 % | < 4 % |
| Flyout open | 2.07 % | 3.29 % | (0.20 %, not valid) | < 4 % |
| Idle in the tray | 0.000 %, 3.5 MB | 0.000 %, 3.5 MB | 0.000 %, 3.5 MB | < 0.1 %, < 25 MB |

The median open state is 3.0 %, inside the budget. Run 3's flyout closed during the measure: another window came to the front, and the flyout now closes then, as it should; that figure measures a closed flyout.

* **Minimising now stops the meters.** Windows reports minimising as a resize to 0 × 0, not as occlusion, so before this the meters kept running behind a minimised window. They stop now, and the window is drawn whole when it comes back.
* **The foreground watch** reads the window in front every 150 ms only while the flyout is open: one cheap call, stopped with the flyout.
* **The speaker** changes its icon only when the number of waves changes: one `NIM_MODIFY`, not one per volume step.
