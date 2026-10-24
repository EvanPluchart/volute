//! Command-line flags. Test flags reach a state directly, for screenshots, benchmarks and tests.

pub const USAGE: &str =
    "usage: volute [--open] [--no-settings] [--no-tray] [--dark|--light] [--mock]";

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
            _ => return Err(format!("unknown argument `{arg}`")),
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
            }
        );
        assert_eq!(parse_all(&[]).unwrap(), Options::default());
        assert_eq!(parse_all(&["--dark"]).unwrap().dark, Some(true));
    }

    #[test]
    fn refuses_unknown_flags() {
        assert!(parse_all(&["--osd"]).is_err());
    }
}
