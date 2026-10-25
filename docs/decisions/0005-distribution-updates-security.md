# 0005 — Distribution, updates and security

**Status:** accepted, 2026-10-24

## Distribution

* An Inno Setup installer, per user (`%LOCALAPPDATA%\Programs\Volute`), no administrator rights.
  * `AppMutex=Local\Volute.SingleInstance` and `CloseApplications=no`.
  * "Start with Windows" offered.
  * A portable zip next to it.
* A release workflow on a `v*` tag builds the installer and the zip and publishes `SHA256SUMS` with them. It drafts the release, and the author publishes it.

## Updates from the app

This is Vigie's design (its decision 0012), proven over four releases:

* **When:** 3 s after start, then every 6 hours, on a thread of its own.
* **What it asks:** one request to `https://github.com/EvanPluchart/volute/releases/latest`, redirect not followed, which names the newest version. There is no API and no rate limit. It can be turned off in Settings, and `--no-update-check` turns it off for test runs.
* **How it shows:**
  * a notification once per version;
  * a line in the flyout and in Settings: "Volute X is available · Update".
* **One click:**
  1. downloads the installer into a fresh folder;
  2. checks it against `SHA256SUMS`, and refuses it without a matching line;
  3. releases the single instance;
  4. runs the installer with `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /relaunch=1`, then quits.
* **The installer** waits up to 10 s for the mutex to clear; failing that, it starts the old Volute again. Then it starts Volute with `--updated`, which shows "Volute X is installed · See what's new".
* **The repository is public by then.** For an anonymous client, a private repository's `releases/latest` gives no redirect, and the updater would find nothing.
* **Where it does not install:**
  * an elevated Volute never runs a downloaded file;
  * a portable copy opens the release page.
* **Which copy is installed** is read from the installer's uninstall key (`InstallLocation` under its `AppId`, per user), as Recto does, not guessed from the path.
* **Test runs never go online** (`--mock`, `--no-settings`, `--no-update-check`). `--pretend-version=X.Y.Z` makes a run behave as an older version, by hand only; `--offer-update=X.Y.Z` shows the offer without any request, for screenshots.

## Security

* **No elevation, ever.** Volute does not need administrator rights: per-app and device volumes, the default device and the keyboard hook all work as a standard user.
* **The keyboard hook** follows decision 0003: volume keys and Volute's shortcuts only, nothing read, kept or sent.
* **Network:** the update check and download, to GitHub only. Nothing else leaves the machine: no telemetry, no crash upload.
* **Files:** settings and profiles in `%APPDATA%\Volute`; downloads in a fresh temporary folder; nothing else is written.
* **Processes:** Volute opens no process handle and reads no other process's memory. App names and icons come from the session's process image path (`QueryFullProcessImageNameW` with limited query rights) and its file's resources.
* **Starting other programs:** Volute starts nothing by name lookup, so a planted file cannot run in its place. The installer, `explorer.exe` and Settings URIs use full paths, or `ShellExecute` of an `ms-settings:` or `https:` URI.
* **Releases:**
  * workflows have a read-only default token, and their actions are pinned by commit;
  * release builds use `--locked`;
  * binaries are signed once a certificate exists; until then, the checksum is the integrity check.
