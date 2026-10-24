//! The Windows backend on a session the test owns: a helper process plays a quiet tone, the
//! test finds that session by its process ID, changes its volume and mute, reads them back, and
//! checks every other session kept its values. Nothing else is ever changed (CLAUDE.md).

#[cfg(windows)]
#[test]
fn sets_volume_and_mute_of_its_own_session_only() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use volute_audio::{Audio, Session, Target, WindowsAudio, default_device};

    // CI runners have no audio endpoint, and may have no audio service at all: nothing to play
    // on, nothing to test. Anywhere else, Core Audio failing is a failure.
    let on_ci = std::env::var_os("CI").is_some();
    let mut devices = Vec::new();
    let mut audio = match WindowsAudio::new().and_then(|mut a| a.devices(&mut devices).map(|()| a)) {
        Ok(audio) => audio,
        Err(err) if on_ci => {
            eprintln!("skipped: no Core Audio on this runner ({err})");
            return;
        }
        Err(err) => panic!("Core Audio: {err}"),
    };
    let Some(device) = default_device(&devices).map(|d| d.id.clone()) else {
        eprintln!("skipped: no default render device on this machine");
        return;
    };

    let mut tone = Command::new(env!("CARGO_BIN_EXE_volute-tone"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the tone helper starts");
    let pid = tone.id();
    let mut line = String::new();
    BufReader::new(tone.stdout.take().expect("piped"))
        .read_line(&mut line)
        .expect("the helper says it plays");
    assert_eq!(line.trim(), "playing", "the helper could not play");

    // Ours, by process ID and nothing else.
    let mut sessions = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    let own = loop {
        audio.sessions(&device, &mut sessions).expect("sessions");
        if let Some(own) = sessions.iter().find(|s| s.pid == pid) {
            break own.clone();
        }
        assert!(Instant::now() < deadline, "no session for the helper (pid {pid})");
        std::thread::sleep(Duration::from_millis(100));
    };
    assert!(own.exe.as_ref().is_some_and(|p| p.ends_with("volute-tone.exe")));
    let others: Vec<Session> = sessions.iter().filter(|s| s.pid != pid).cloned().collect();

    let target = Target::Session(own.id.clone());
    let read = |audio: &mut WindowsAudio| {
        let mut sessions = Vec::new();
        audio.sessions(&device, &mut sessions).expect("sessions");
        sessions
    };
    let find = |sessions: &[Session]| sessions.iter().find(|s| s.id == own.id).cloned().expect("our session");

    // Windows remembers a volume per executable: start from whatever it holds, restore it after.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let audio = &mut audio;
        audio.set_volume(&target, 0.37).expect("set_volume");
        audio.set_mute(&target, true).expect("set_mute");
        let now = find(&read(audio));
        assert!((now.volume - 0.37).abs() < 1e-4, "volume read back: {}", now.volume);
        assert!(now.muted, "mute read back");
        audio.set_mute(&target, false).expect("set_mute");
        audio.set_volume(&target, 0.81).expect("set_volume");
        let now = find(&read(audio));
        assert!((now.volume - 0.81).abs() < 1e-4, "volume read back: {}", now.volume);
        assert!(!now.muted, "unmute read back");

        // Every other session still present kept its volume and mute.
        let after = read(audio);
        for before in &others {
            if let Some(now) = after.iter().find(|s| s.id == before.id) {
                assert_eq!(
                    (now.volume, now.muted),
                    (before.volume, before.muted),
                    "session of pid {} changed",
                    before.pid
                );
            }
        }
    }));
    let _ = audio.set_volume(&target, own.volume);
    let _ = audio.set_mute(&target, own.muted);
    // Closing its input ends the helper; it is ours to stop.
    drop(tone.stdin.take());
    let deadline = Instant::now() + Duration::from_secs(5);
    while tone.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = tone.kill();
    let _ = tone.wait();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
