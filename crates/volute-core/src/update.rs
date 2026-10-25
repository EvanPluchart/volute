//! Updates from the app (decision 0005): which release is newer, where its installer is, and
//! the checksum it must have. No I/O here; `volute-shell` fetches, `volute-app` decides.

use std::fmt;

/// The public half of the release signing key (decision 0010): ECDSA P-256, the point's x then
/// y, in hex. `SHA256SUMS.sig` must be its signature of `SHA256SUMS`, or no checksum in it is
/// trusted. `cargo xtask dist` refuses to sign with any other key.
pub const RELEASE_KEY: &str = "13565c39884c1e6186963a46e01b05a9184e6a95e2ded28643083a034f58c69d41d9f936931a0d160ba44756c83ce127c80833a86c46057e308f03c1f0b6df9b";

/// The release's checksums, and their signature.
pub const SUMS: &str = "SHA256SUMS";
pub const SUMS_SIGNATURE: &str = "SHA256SUMS.sig";

/// `RELEASE_KEY` as bytes.
pub fn release_key() -> [u8; 64] {
    parse_hex(RELEASE_KEY).expect("RELEASE_KEY is 128 hex digits")
}

/// The signature in `SHA256SUMS.sig`: 128 hex digits (r then s), then an optional line end.
pub fn parse_signature(text: &str) -> Option<[u8; 64]> {
    parse_hex(text.trim())
}

/// Where GitHub redirects to the newest published release.
pub const LATEST_URL: &str = "https://github.com/EvanPluchart/volute/releases/latest";
/// The release page of `version`, for a copy that cannot install itself.
pub fn release_page(version: Version) -> String {
    format!("https://github.com/EvanPluchart/volute/releases/tag/v{version}")
}

/// A release version, `major.minor.patch`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// `0.1.2` or `v0.1.2`. Anything else (a pre-release, a stray suffix) is `None`: Volute
    /// offers only plain releases.
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.trim();
        let text = text.strip_prefix('v').unwrap_or(text);
        let mut parts = text.split('.');
        let mut next = || -> Option<u32> {
            let part = parts.next()?;
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            part.parse().ok()
        };
        let version = Version {
            major: next()?,
            minor: next()?,
            patch: next()?,
        };
        parts.next().is_none().then_some(version)
    }

    /// The version a `/releases/tag/vX.Y.Z` address names (GitHub's redirect for the latest).
    pub fn from_release_url(url: &str) -> Option<Version> {
        let (_, tag) = url.trim().split_once("/releases/tag/")?;
        Version::parse(tag.trim_end_matches('/'))
    }

    /// The Windows installer's file name in the release, as `cargo xtask dist` names it.
    pub fn setup_name(self) -> String {
        format!("Volute-{self}-setup.exe")
    }

    /// Where a file of this release downloads from.
    pub fn download_url(self, file: &str) -> String {
        format!("https://github.com/EvanPluchart/volute/releases/download/v{self}/{file}")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// `latest` when it is strictly newer than `current`; never a downgrade or the same version.
pub fn newer(current: &str, latest: Version) -> Option<Version> {
    (Version::parse(current)? < latest).then_some(latest)
}

/// The SHA-256 that `SHA256SUMS` (`sha256sum` output) gives for `file`.
pub fn checksum_for(sums: &str, file: &str) -> Option<[u8; 32]> {
    sums.lines().find_map(|line| {
        let (hex, name) = line.trim().split_once(char::is_whitespace)?;
        // `sha256sum` marks binary mode with a `*` before the name.
        let name = name.trim_start().trim_start_matches('*');
        (name == file).then(|| parse_hex(hex)).flatten()
    })
}

/// `N` bytes from exactly `2 * N` hex digits (no sign, no prefix).
fn parse_hex<const N: usize>(hex: &str) -> Option<[u8; N]> {
    if hex.len() != N * 2 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; N];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn versions_compare_by_number() {
        assert_eq!(v("v0.1.2"), v("0.1.2"));
        assert!(v("0.1.10") > v("0.1.9"));
        assert!(v("1.0.0") > v("0.99.99"));
        for bad in [
            "",
            "v",
            "0.1",
            "0.1.2.3",
            "0.1.2-beta",
            "0.1.x",
            "0..1",
            " v0.1.+2",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn only_a_newer_release_is_offered() {
        assert_eq!(newer("0.1.1", v("0.1.2")), Some(v("0.1.2")));
        assert_eq!(newer("0.1.2", v("0.1.2")), None);
        assert_eq!(newer("0.2.0", v("0.1.9")), None);
        assert_eq!(newer("not a version", v("9.9.9")), None);
    }

    #[test]
    fn the_latest_redirect_names_the_version() {
        assert_eq!(
            Version::from_release_url("https://github.com/EvanPluchart/volute/releases/tag/v0.1.1"),
            Some(v("0.1.1"))
        );
        // No release yet: GitHub sends back to the releases list.
        assert_eq!(
            Version::from_release_url("https://github.com/EvanPluchart/volute/releases"),
            None
        );
        assert_eq!(v("0.1.2").setup_name(), "Volute-0.1.2-setup.exe");
        assert_eq!(
            v("0.1.2").download_url("SHA256SUMS"),
            "https://github.com/EvanPluchart/volute/releases/download/v0.1.2/SHA256SUMS"
        );
    }

    #[test]
    fn signatures_and_the_key_are_hex() {
        assert_eq!(release_key().len(), 64);
        let sig = "ab".repeat(64);
        assert_eq!(parse_signature(&format!("{sig}\r\n")), Some([0xab; 64]));
        assert_eq!(parse_signature(&sig[..126]), None);
        assert_eq!(parse_signature(&format!("+{}", &sig[1..])), None);
    }

    #[test]
    fn checksums_are_read_from_sha256sum_output() {
        let a = "a".repeat(64);
        let sums = format!(
            "{a}  Volute-0.1.2-portable.zip\n{}  Volute-0.1.2-setup.exe\n{}  *Volute-0.1.2-other.exe\n",
            "0f".repeat(32),
            "1e".repeat(32)
        );
        assert_eq!(
            checksum_for(&sums, "Volute-0.1.2-setup.exe"),
            Some([0x0f; 32])
        );
        assert_eq!(
            checksum_for(&sums, "Volute-0.1.2-other.exe"),
            Some([0x1e; 32])
        );
        assert_eq!(
            checksum_for(&sums, "Volute-0.1.3-setup.exe"),
            None
        );
        assert_eq!(
            checksum_for(
                "xyz  Volute-0.1.2-setup.exe",
                "Volute-0.1.2-setup.exe"
            ),
            None
        );
    }
}
