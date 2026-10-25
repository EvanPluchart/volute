# 0005 — Distribution, updates and security

**Status:** accepted, 2026-10-24; aligned with the code and completed by decision 0010 on 2026-10-25.

## Distribution

* An Inno Setup installer, per user (`%LOCALAPPDATA%\Programs\Volute`), no administrator rights.
  * `AppMutex=Local\Volute.SingleInstance` and `CloseApplications=no`.
  * "Start with Windows" offered.
  * A portable zip next to it.
* `cargo xtask dist` builds the installer and the zip on the author's machine and writes `SHA256SUMS` and its signature (decisions 0006 and 0010). The author attaches the four files to a draft release and publishes it. There is no release workflow.

## Updates from the app

This is Vigie's design (its decision 0012), proven over four releases:

* **When:** 3 s after start, then every 6 hours, on a thread of its own.
* **What it asks:** one request to `https://github.com/EvanPluchart/volute/releases/latest`, redirect not followed, which names the newest version. There is no API and no rate limit. It can be turned off in Settings, and `--no-update-check` turns it off for test runs.
* **How it shows:**
  * a notification once per version;
  * a line in the flyout, the main window's rail and Settings: "Volute X is available · Update".
* **One click:**
  1. downloads `SHA256SUMS`, its signature and the installer into a fresh folder under `%LOCALAPPDATA%\Volute\Updates`;
  2. checks that `SHA256SUMS` is signed with the release key (decision 0010), then the installer against its line, from a handle that stops anyone from replacing the file before it runs; an unsigned release or a missing line is refused;
  3. releases the single instance;
  4. runs the installer with `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /NOCLOSEAPPLICATIONS /relaunch=1`, then quits.
* **Only forwards:** the button installs only a version strictly newer than the running one, whatever the view shows.
* **The installer** waits up to 10 s for the mutex to clear; failing that, it starts the old Volute again. Then it starts Volute with `--updated`, which shows "Volute X is installed · What's new".
* **The repository is public by then.** For an anonymous client, a private repository's `releases/latest` gives no redirect, and the updater would find nothing.
* **Where it does not install:**
  * an elevated Volute never runs a downloaded file;
  * a portable copy opens the release page.
* **Which copy is installed** is read from the installer's uninstall key (`InstallLocation` under its `AppId`, per user), as Recto does, not guessed from the path.
* **Test runs stay offline** (`--mock`, `--no-settings`, `--no-update-check`). The one exception is `--pretend-version=X.Y.Z`, which makes a run behave as an older version and does go online, by hand only. `--offer-update=X.Y.Z` shows the offer without any request, for screenshots.

## Security

* **No elevation, ever.** Volute does not need administrator rights: per-app and device volumes, the default device and the keyboard hook all work as a standard user.
* **The keyboard hook** follows decision 0003: volume keys and Volute's shortcuts only, nothing read, kept or sent.
* **Network:** the update check and download, to GitHub only. Nothing else leaves the machine: no telemetry, no crash upload.
* **Files:** settings and profiles in `%APPDATA%\Volute`; update downloads in `%LOCALAPPDATA%\Volute\Updates`, removed at the next start; nothing else is written.
* **Processes:** Volute reads no other process's memory. The only process handles it opens have limited query rights, to read an image path (`QueryFullProcessImageNameW`): a sound's app, for its name and icon from its file's resources, and the running Volute a second launch signals, to check it is the same program.
* **Starting other programs:** Volute starts nothing by name lookup, so a planted file cannot run in its place. The installer, `explorer.exe` and Settings URIs use full paths, or `ShellExecute` of an `ms-settings:` or `https:` URI.
* **Releases:**
  * built locally with `--locked`, from a clean `main`;
  * `SHA256SUMS` is signed with the release key, whose private half never leaves the author's machine (decision 0010);
  * binaries are signed once a certificate exists.
