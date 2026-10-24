# Initial prompt: milestone M0

Paste this into a new agent session at the repository root.

---

You are building **Volute**, a volume mixer for Windows in Rust with Slint. Read `CLAUDE.md`, `docs/ARCHITECTURE.md`, `docs/ROADMAP.md`, `docs/BRAND.md` and every record in `docs/decisions/` first. Then deliver **milestone M0** exactly as the roadmap defines it, and nothing beyond it.

`../vigie` is the author's task manager, built with the same toolkit and the same rules. Reuse its patterns where they fit, adapting rather than copying blindly:

* `slint` setup and pinning, `@tr` with the French translation test;
* the sampler thread sending frames with `invoke_from_event_loop` and patching models;
* the tray and the single instance (`vigie-shell`);
* the exit watchdog and the `quits_on_close` test;
* the settings file;
* `cargo xtask bench-overhead`.

## Deliverables

1. **Workspace.**
   * Crates `volute-audio`, `volute-core`, `volute-shell`, `volute-app` (binary `volute`), and `xtask`, with the dependency rules of `CLAUDE.md`.
   * Pin `slint` and `slint-build` to the version Vigie uses. Pin the `windows` crate (check crates.io for the current version, with the Core Audio features it needs), plus `flume`, `anyhow` and `thiserror`.
   * Add the `xtask` alias in `.cargo/config.toml`, and a release profile like Vigie's.
2. **`volute-audio`:** the `Audio` trait and types from ARCHITECTURE.md, with:
   * **the Windows backend.** COM on the audio thread, as a multithreaded apartment. It covers:
     * render devices with volume and mute;
     * the sessions of the default render device, with process ID, executable path, display name, volume, mute, state, and whether it is the system-sounds session;
     * peaks, and `set_volume` and `set_mute` for sessions and devices.

     No events yet (M1): the audio thread re-reads the sessions every second while the window is open.
   * **`MockAudio`** with scripted devices, sessions and peaks.
3. **`volute-core`:**
   * sessions grouped into one row per app (by executable path);
   * the perceptual volume curve (slider position ↔ Windows scalar), with tests: monotonic, exact at 0 and 1, round-trips.
4. **`volute-app`:** `volute [--open] [--no-settings] [--no-tray] [--dark|--light] [--mock]`.
   * The window shows one row per app: its name, a slider, a mute button and a live meter at 30 Hz.
   * Dragging the slider sets that app's volume.
   * `--mock` runs on `MockAudio`, for screenshots and tests.
   * The audio thread sends frames to the UI with at most one in flight, and models are patched, never replaced.
   * An About window shows `AboutSlint`.
   * Use plain Slint controls styled from a first `ui/theme.slint`: the real design comes in the design track.
5. **Lifecycle (decision 0004), from day one:**
   * an explicit `Quit` to the audio thread;
   * the exit watchdog armed first;
   * the single instance, with an acknowledgement and a 2.5 s fallback, taken except under `--no-settings`;
   * an integration test that closes the window with `taskkill` (no `/F`) and checks the process ends.
6. **`xtask bench-overhead`:** builds in release and runs Volute with `--mock` for 60 s, window open, then idle. It prints CPU % of one core and peak private memory for each.
7. **CI:** `.github/workflows/ci.yml` on `windows-latest`, running `cargo check`, `cargo test` and `cargo clippy -- -D warnings` (all with `--locked`). Configure it as follows:
   * triggers on pull requests and pushes to `main`, plus `workflow_dispatch`, so pushes to `dev` stay manual;
   * a read-only default token;
   * actions pinned by commit SHA;
   * a budget job that reports without failing the build.
8. **Tests:**
   * grouping by app;
   * the volume curve;
   * `MockAudio` round-trips;
   * frame patching produces the expected rows;
   * the close test above;
   * **the real backend on a session the test owns.** The test spawns a helper process that plays a short, quiet tone. It finds that session through the Windows backend, calls `set_volume` and `set_mute` on it, reads the values back, and checks that every other session kept its volume and mute.

   Tests never change the volume of anything they did not start. No agent drags a slider on the user's apps either: synthetic input stays restricted (`CLAUDE.md`).

## Rules

* Respect the hard rules in `CLAUDE.md` and the records in `docs/decisions/`. If a record turns out to be wrong in practice, stop and explain instead of working around it.
* Work on `dev`. Commit in small steps with conventional messages (`feat(audio): …`), and push to `origin/dev` when done. No AI attribution lines.
* No logo, OSD or keyboard hook in M0. They belong to later milestones.

## Done when

* `cargo run -p volute -- --open` lists the apps playing sound on this machine. That is a look only: change no real app's volume. The owned-session test proves `set_volume` and `set_mute` work, and `--mock` screenshots taken with PrintWindow show the window.
* Closing the window ends the process, and the test proves it.
* `cargo xtask bench-overhead` prints its numbers.
* CI is green, run once by hand.
* You report:
  * what works, and what you could not verify;
  * the overhead numbers;
  * anything you changed in the docs.
