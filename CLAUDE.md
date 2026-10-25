# Volute — agent guide

Volume mixer for Windows in Rust with Slint. Read these before changing code:

1. `docs/ARCHITECTURE.md`: layers, threading, audio model, performance budget, conventions.
2. `docs/ROADMAP.md`: current milestone and its exit criteria. Work on the current milestone only.
3. `docs/decisions/`: settled choices. Do not reopen them without a new decision record.
4. `docs/BRAND.md`: art direction and design tokens, before touching any `.slint` file.

## Hard rules

* `volute-audio`, `volute-core` and `volute-shell` never depend on `slint`. `volute-core` and `volute-app` never call the OS: audio goes through `volute-audio`, desktop actions (tray, hotkeys, OSD window placement, files, network) through `volute-shell`.
* Only the audio thread touches COM audio objects. The UI thread only patches models.
* Respect the performance budget in `docs/ARCHITECTURE.md`. A change that breaks it does not merge.
* **Closing must end the process.** Every thread stops on an explicit quit, and an exit watchdog is armed first. Volute lives in the tray, so a hidden window is normal, but a process with no window and no tray icon is a bug (decision 0004).
* **The keyboard hook only ever looks at volume keys.** It never records, logs or forwards any other key, and it returns at once (decision 0003).
* Never change the volume of an app or device you did not create in a test, except through the app at the user's request. Tests use their own audio session (a sound the test plays) or mocks.
* Slint is pinned. Upgrade it only when asked, in a dedicated commit.
* Code, comments, commits in English. UI strings through `@tr()`, with French translations (a test fails on a missing one).
* Work on `dev`. Never push to `main`. No `Co-Authored-By` or AI attribution lines in commits or PRs.

## Commands

```sh
cargo check --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo run -p volute -- --open --no-settings
cargo xtask check          # check, test and clippy, all --locked: run before every push (no hosted CI, decision 0006)
cargo xtask bench-overhead
cargo xtask dist           # installer, portable zip, SHA256SUMS and its signature into dist/ (decisions 0005, 0010); uploads nothing
```

## Seeing the app

Launch with test flags (`--open`, `--mock`, `--flyout[=center]`, `--tab=mixer|devices|settings|about`, `--dark`/`--light`, `--lang=en|fr`, `--no-settings`, `--no-tray`, `--no-update-check`, `--offer-update=X.Y.Z`; `--osd` and `--no-hook` come with M2) to reach a state directly. Screenshots for anything public use `--mock`, never the user's real apps. The release signing key lives outside the repository (decision 0010): never copy, print or commit it. On Windows, capture windows with PrintWindow, not a screen grab. Never send synthetic input unless the Volute window is verified to be in the foreground. Test runs never go online and never install a keyboard hook unless the test is about the hook.

## Skills

The `logo-design` skill (installed globally, not part of this repository) designs the logo and app icon. Brief in `docs/BRAND.md`.
