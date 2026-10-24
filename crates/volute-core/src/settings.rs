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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub theme: Theme,
    /// Lines this version does not know, written back as they were.
    unknown: Vec<String>,
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
                match (key.trim(), value.trim()) {
                    ("theme", value) => {
                        if let Some(theme) = Theme::parse(value) {
                            settings.theme = theme;
                        }
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
        let mut text = format!("theme = {}\n", self.theme.as_str());
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
        let settings = Settings::parse("# comment\ntheme = dark\nfuture = 3\n");
        assert_eq!(settings.theme, Theme::Dark);
        assert_eq!(settings.to_text(), "theme = dark\nfuture = 3\n");
        assert_eq!(Settings::parse(&settings.to_text()), settings);
    }

    #[test]
    fn bad_values_keep_their_default() {
        assert_eq!(Settings::parse("theme = purple").theme, Theme::System);
        assert_eq!(Settings::parse("\u{0}garbage").theme, Theme::System);
    }
}
