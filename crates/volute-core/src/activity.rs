//! Apps that stopped playing fade out of the mixer after a while (docs/ROADMAP.md, M1).
//!
//! An app stays listed while it plays and for `LINGER` after it last played, long enough to
//! pause a video and come back to adjust it. System sounds always stay: Windows plays them at
//! any moment. The clock is passed in (time since start), so the rules are tested without
//! waiting.

use std::collections::HashMap;
use std::time::Duration;

use crate::apps::{App, AppKey};

/// How long an app stays listed after it last played.
pub const LINGER: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
pub struct Activity {
    /// When each app was last seen playing (or first seen at all).
    last: HashMap<AppKey, Duration>,
}

impl Activity {
    /// Records which of `apps` play at `now`, and forgets apps no longer present.
    pub fn update(&mut self, apps: &[App], now: Duration) {
        self.last.retain(|key, _| apps.iter().any(|a| &a.key == key));
        for app in apps {
            let last = self.last.entry(app.key.clone()).or_insert(now);
            if app.playing {
                *last = now;
            }
        }
    }

    /// Whether `app` is still listed at `now`.
    pub fn shown(&self, app: &App, now: Duration) -> bool {
        app.playing
            || app.system_sounds()
            || self.last.get(&app.key).is_none_or(|&last| now.saturating_sub(last) < LINGER)
    }

    /// The next moment an app listed now leaves the list, if any: the mixer checks again then.
    pub fn next_expiry(&self, apps: &[App], now: Duration) -> Option<Duration> {
        apps.iter()
            .filter(|a| !a.playing && !a.system_sounds())
            .filter_map(|a| self.last.get(&a.key).map(|&last| last + LINGER))
            .filter(|&at| at > now)
            .min()
    }
}

#[cfg(test)]
mod tests {
    use volute_audio::SessionId;

    use super::*;

    fn app(key: &str, playing: bool) -> App {
        App {
            key: if key == "system" { AppKey::SystemSounds } else { AppKey::Exe(key.into()) },
            name: key.into(),
            volume: 1.0,
            muted: false,
            playing,
            sessions: vec![SessionId(key.into())],
        }
    }

    const S: fn(u64) -> Duration = Duration::from_secs;

    #[test]
    fn a_silent_app_lingers_then_leaves() {
        let mut activity = Activity::default();
        let playing = [app("music", true)];
        activity.update(&playing, S(0));
        let paused = [app("music", false)];
        activity.update(&paused, S(10));
        assert!(activity.shown(&paused[0], S(10)));
        assert!(activity.shown(&paused[0], S(59)), "still there just under a minute after");
        assert!(!activity.shown(&paused[0], S(60)), "gone a minute after it last played");
        assert_eq!(activity.next_expiry(&paused, S(10)), Some(S(60)));
        // It plays again: back, and the clock starts over.
        activity.update(&playing, S(70));
        activity.update(&paused, S(80));
        assert!(activity.shown(&paused[0], S(100)));
    }

    #[test]
    fn an_app_first_seen_silent_gets_the_same_grace() {
        let mut activity = Activity::default();
        let apps = [app("paused-video", false)];
        activity.update(&apps, S(5));
        assert!(activity.shown(&apps[0], S(30)));
        assert!(!activity.shown(&apps[0], S(65)));
    }

    #[test]
    fn system_sounds_always_stay() {
        let mut activity = Activity::default();
        let apps = [app("system", false)];
        activity.update(&apps, S(0));
        assert!(activity.shown(&apps[0], S(3600)));
        assert_eq!(activity.next_expiry(&apps, S(0)), None);
    }

    #[test]
    fn apps_that_left_are_forgotten() {
        let mut activity = Activity::default();
        activity.update(&[app("a", false)], S(0));
        activity.update(&[], S(1));
        // Back later: a fresh grace period, not the old timestamp.
        let again = [app("a", false)];
        activity.update(&again, S(500));
        assert!(activity.shown(&again[0], S(520)));
    }
}
