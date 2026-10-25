//! User settings and their file format (decision 0004): plain `key = value` lines. Parsing never
//! fails: a bad value keeps its default, and unknown keys are kept, so a newer Volute's settings
//! survive an older one. No file access here.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    System,
    Dark,
    Light,
}

impl Theme {
    fn parse(value: &str) -> Option<Theme> {
        match value {
            "system" => Some(Theme::System),
            "dark" => Some(Theme::Dark),
            "light" => Some(Theme::Light),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub theme: Theme,
    /// Closing the main window leaves Volute in the tray instead of quitting (decision 0009), the
    /// default. Saved as `keep_in_tray`.
    pub close_to_tray: bool,
    /// Look for a new release at start and every 6 hours (decision 0005).
    pub check_updates: bool,
    /// Lines this version does not know, written back as they were.
    unknown: Vec<String>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            theme: Theme::System,
            close_to_tray: true,
            check_updates: true,
            unknown: Vec::new(),
        }
    }
}

fn flag(value: &str) -> Option<bool> {
    match value {
        "true" | "on" | "1" => Some(true),
        "false" | "off" | "0" => Some(false),
        _ => None,
    }
}

impl Settings {
    pub fn parse(text: &str) -> Settings {
        let mut settings = Settings::default();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }
            let known = trimmed.split_once('=').is_some_and(|(key, value)| {
                let value = value.trim();
                match key.trim() {
                    "theme" => {
                        settings.theme = Theme::parse(value).unwrap_or(settings.theme);
                        true
                    }
                    "keep_in_tray" => {
                        settings.close_to_tray = flag(value).unwrap_or(settings.close_to_tray);
                        true
                    }
                    // 0.0.1 saved its default (quit) with every change: dropped, so the new default
                    // (stay in the tray) applies.
                    "close_to_tray" => true,
                    "check_updates" => {
                        settings.check_updates = flag(value).unwrap_or(settings.check_updates);
                        true
                    }
                    _ => false,
                }
            });
            if !known {
                settings.unknown.push(trimmed.to_owned());
            }
        }
        settings
    }

    pub fn to_text(&self) -> String {
        let mut text = format!(
            "theme = {}\nkeep_in_tray = {}\ncheck_updates = {}\n",
            self.theme.as_str(),
            self.close_to_tray,
            self.check_updates
        );
        for line in &self.unknown {
            text.push_str(line);
            text.push('\n');
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_values_and_keeps_unknown_keys() {
        let settings = Settings::parse("# comment\ntheme = dark\nkeep_in_tray = off\nfuture = 3\n");
        assert_eq!(settings.theme, Theme::Dark);
        assert!(!settings.close_to_tray);
        assert!(settings.check_updates);
        assert_eq!(
            settings.to_text(),
            "theme = dark\nkeep_in_tray = false\ncheck_updates = true\nfuture = 3\n"
        );
        assert_eq!(Settings::parse(&settings.to_text()), settings);
    }

    #[test]
    fn bad_values_keep_their_default() {
        let settings = Settings::parse("theme = purple\nkeep_in_tray = maybe\ncheck_updates = 2");
        assert_eq!(settings, Settings { unknown: vec![], ..Settings::default() });
        assert_eq!(Settings::parse("\u{0}garbage").theme, Theme::System);
    }

    #[test]
    fn stays_in_the_tray_unless_told_otherwise() {
        assert!(Settings::default().close_to_tray);
        // What 0.0.1 wrote, its default included, is dropped.
        let old = Settings::parse("theme = light\nclose_to_tray = false\ncheck_updates = true\n");
        assert!(old.close_to_tray);
        assert!(!old.to_text().contains("close_to_tray"));
    }
}
