//! Command-line flags. Test flags reach a state directly, for screenshots, benchmarks and tests.

pub const USAGE: &str = "usage: volute [--open] [--no-settings] [--no-tray] [--dark|--light] [--mock] [--flyout] \
     [--tab=mixer|devices|settings|about] [--about] [--no-update-check] [--pretend-version=X.Y.Z] [--updated] [--offer-update=X.Y.Z]";

/// The main window's views, as `AppWindow.tab` numbers them.
pub const TAB_MIXER: i32 = 0;
pub const TAB_DEVICES: i32 = 1;
pub const TAB_SETTINGS: i32 = 2;
pub const TAB_ABOUT: i32 = 3;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Shows the window at start instead of waiting in the tray.
    pub open: bool,
    /// Reads no settings file and takes no single instance: defaults plus flags, next to the
    /// user's own Volute (tests, benchmarks, screenshots).
    pub no_settings: bool,
    /// Creates no tray icon: nothing left behind when killed. The window then shows at start.
    pub no_tray: bool,
    /// `None` keeps the theme from the settings.
    pub dark: Option<bool>,
    /// Runs on the scripted `MockAudio` instead of Core Audio (screenshots, tests).
    pub mock: bool,
    /// Opens the flyout at start, at the bottom right (screenshots).
    pub flyout: bool,
    /// The main window's view at start (`TAB_*`).
    pub tab: i32,
    /// Never looks for a new release in this run (decision 0005).
    pub no_update_check: bool,
    /// Behaves as this older version, to see an update offered (`--pretend-version=0.0.1`).
    /// The one way a test run goes online, so only by hand.
    pub pretend_version: Option<String>,
    /// Started again by the installer after an update from the app.
    pub updated: bool,
    /// Shows this version as available without asking GitHub (screenshots).
    pub offer_update: Option<String>,
}

impl Options {
    /// A run that sets a state up (tests, benchmarks, screenshots): no single instance, no
    /// remembered volumes, nothing written, nothing changed on the machine.
    pub fn test_run(&self) -> bool {
        self.no_settings || self.mock
    }

    /// Whether this run may look for updates at all; the setting decides after that. A test run
    /// never goes online unless it pretends to be an older version.
    pub fn may_check_updates(&self) -> bool {
        !self.no_update_check && (self.pretend_version.is_some() || !self.test_run())
    }
}

pub fn parse(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut options = Options::default();
    for arg in args {
        match arg.as_str() {
            "--open" => options.open = true,
            "--no-settings" => options.no_settings = true,
            "--no-tray" => options.no_tray = true,
            "--dark" => options.dark = Some(true),
            "--light" => options.dark = Some(false),
            "--mock" => options.mock = true,
            "--flyout" => options.flyout = true,
            "--no-update-check" => options.no_update_check = true,
            "--updated" => options.updated = true,
            "--tab=mixer" => options.tab = TAB_MIXER,
            "--tab=devices" => options.tab = TAB_DEVICES,
            "--tab=settings" => options.tab = TAB_SETTINGS,
            "--tab=about" | "--about" => {
                options.tab = TAB_ABOUT;
                options.open = true;
            }
            _ => {
                let version = |flag: &str| {
                    arg.strip_prefix(flag)
                        .filter(|v| volute_core::update::Version::parse(v).is_some())
                        .map(str::to_owned)
                };
                if let Some(v) = version("--pretend-version=") {
                    options.pretend_version = Some(v);
                } else if let Some(v) = version("--offer-update=") {
                    options.offer_update = Some(v);
                } else {
                    return Err(format!("unknown argument `{arg}`"));
                }
            }
        }
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_all(args: &[&str]) -> Result<Options, String> {
        parse(args.iter().map(|a| (*a).to_owned()))
    }

    #[test]
    fn reads_every_flag() {
        let options = parse_all(&["--open", "--no-settings", "--no-tray", "--light", "--mock"]).unwrap();
        assert_eq!(
            options,
            Options {
                open: true,
                no_settings: true,
                no_tray: true,
                dark: Some(false),
                mock: true,
                ..Options::default()
            }
        );
        assert_eq!(parse_all(&[]).unwrap(), Options::default());
        assert_eq!(parse_all(&["--dark"]).unwrap().dark, Some(true));
        assert_eq!(parse_all(&["--tab=devices"]).unwrap().tab, TAB_DEVICES);
        assert_eq!(parse_all(&["--tab=settings"]).unwrap().tab, TAB_SETTINGS);
        let about = parse_all(&["--about"]).unwrap();
        assert!(about.open && about.tab == TAB_ABOUT);
        let updated = parse_all(&["--updated", "--pretend-version=0.0.1"]).unwrap();
        assert!(updated.updated);
        assert_eq!(updated.pretend_version.as_deref(), Some("0.0.1"));
        assert_eq!(parse_all(&["--offer-update=0.2.0"]).unwrap().offer_update.as_deref(), Some("0.2.0"));
    }

    #[test]
    fn refuses_unknown_flags() {
        assert!(parse_all(&["--osd"]).is_err());
        assert!(parse_all(&["--pretend-version=soon"]).is_err());
    }

    #[test]
    fn test_runs_never_go_online() {
        assert!(parse_all(&[]).unwrap().may_check_updates());
        assert!(!parse_all(&["--no-update-check"]).unwrap().may_check_updates());
        assert!(!parse_all(&["--mock"]).unwrap().may_check_updates());
        assert!(!parse_all(&["--no-settings", "--open"]).unwrap().may_check_updates());
        // By hand only: an older version, to see the offer.
        assert!(parse_all(&["--mock", "--pretend-version=0.0.1"]).unwrap().may_check_updates());
        assert!(!parse_all(&["--pretend-version=0.0.1", "--no-update-check"]).unwrap().may_check_updates());
    }
}
