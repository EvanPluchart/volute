# 0010 — Signed releases

**Status:** accepted, 2026-10-25. Completes decision 0005 (updates).

## Context

The updater downloads `SHA256SUMS` and the installer from the same GitHub release, over the same TLS connection, and runs the installer silently after one click. The checksum catches a truncated or damaged download, but nothing proves who published the release: a stolen GitHub token, or a machine whose TLS traffic is inspected by an extra root certificate, could serve a different installer with a matching checksum (security audit, 2026-10-25, finding 1). Code signing would answer this, but there is no certificate yet.

## Decision

* **A release signing key, ECDSA P-256, made and used with Windows' own BCrypt**, as the rest of the updater: no new crate.
* **The private key never enters the repository.** `cargo xtask release-key` makes it once, in `%USERPROFILE%\.volute-release\signing-key.bin` (or where `VOLUTE_SIGNING_KEY` says). The author keeps a copy offline.
* **The public key is built into Volute**, `RELEASE_KEY` in `volute-core::update`.
* **`cargo xtask dist` signs `SHA256SUMS`** into `SHA256SUMS.sig` (the 64-byte signature in hex), and refuses to sign with a key whose public half is not `RELEASE_KEY`.
* **The updater trusts no checksum without a valid signature.** A missing or wrong `SHA256SUMS.sig` refuses the update and says so ("not signed with Volute's key").
* **When a code-signing certificate exists**, the installer is signed too and the updater checks it as well (`WinVerifyTrust`); the release key stays.

## Consequences

* A release is four files: the installer, the portable zip, `SHA256SUMS` and `SHA256SUMS.sig`.
* **Losing the private key** means the installed Volutes refuse every later update: their users install the next version by hand once, and that version carries a new key. Changing the key before the first release costs nothing.
* The key was made on 2026-10-25, before any release; the author can replace it (`cargo xtask release-key` after moving the old file away, then the new public key in `volute-core`) until the first release is published.
* `cargo test -p volute -- --ignored local_dist` checks a built `dist/` the way the updater will.
