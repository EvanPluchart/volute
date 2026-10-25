//! The Windows backend on a session the test owns: a helper process plays a quiet tone, the
//! test finds that session by its process ID, changes its volume and mute, reads them back, and
//! checks every other session kept its values. Nothing else is ever changed (CLAUDE.md).
//!
//! Events too: listening starts before the tone, so its new session must be announced, and the
//! test's own changes must come back marked as Volute's (decision 0002's echo rule).

#[cfg(windows)]
#[test]
fn sets_volume_and_mute_of_its_own_session_only() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use volute_audio::{
        Audio, AudioEvent, Flow, Session, SessionId, Target, WindowsAudio, default_device,
    };

    // CI-like machines may have no audio endpoint, or no audio service at all: nothing to play
    // on, nothing to test. Anywhere else, Core Audio failing is a failure.
    let lenient = std::env::var_os("CI").is_some();
    let events = Arc::new(Mutex::new(Vec::<AudioEvent>::new()));
    let sink = events.clone();
    let mut devices = Vec::new();
    let started = WindowsAudio::new().and_then(|mut a| {
        a.listen(Arc::new(move |e| sink.lock().unwrap().push(e)))?;
        a.devices(&mut devices).map(|()| a)
    });
    let mut audio = match started {
        Ok(audio) => audio,
        Err(err) if lenient => {
            eprintln!("skipped: no Core Audio here ({err})");
            return;
        }
        Err(err) => panic!("Core Audio: {err}"),
    };
    let Some(device) = default_device(&devices, Flow::Render).map(|d| d.id.clone()) else {
        eprintln!("skipped: no default render device on this machine");
        return;
    };
    // Reading the sessions arms the new-session notifications.
    let mut sessions = Vec::new();
    audio.sessions(&device, &mut sessions).expect("sessions");

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

    let wait_for = |what: &str, found: &dyn Fn(&[AudioEvent]) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !found(&events.lock().unwrap()) {
            assert!(Instant::now() < deadline, "no event: {what}");
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    wait_for("the tone's new session", &|e| {
        e.iter().any(|e| matches!(e, AudioEvent::SessionAdded { device: d } if *d == device))
    });

    // Ours, by process ID and nothing else.
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
    let ours = |id: SessionId, volume: f32| {
        move |e: &[AudioEvent]| {
            e.iter().any(|e| matches!(e,
                AudioEvent::SessionVolume { id: i, volume: v, ours: true, .. } if *i == id && (v - volume).abs() < 1e-4))
        }
    };

    // Windows remembers a volume per executable: start from whatever it holds, restore it after.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let audio = &mut audio;
        audio.set_volume(&target, 0.37).expect("set_volume");
        audio.set_mute(&target, true).expect("set_mute");
        let now = find(&read(audio));
        assert!((now.volume - 0.37).abs() < 1e-4, "volume read back: {}", now.volume);
        assert!(now.muted, "mute read back");
        wait_for("our own volume change, marked as ours", &ours(own.id.clone(), 0.37));
        audio.set_mute(&target, false).expect("set_mute");
        audio.set_volume(&target, 0.81).expect("set_volume");
        let now = find(&read(audio));
        assert!((now.volume - 0.81).abs() < 1e-4, "volume read back: {}", now.volume);
        assert!(!now.muted, "unmute read back");
        wait_for("the second change", &ours(own.id.clone(), 0.81));

        // From a command to the volume applied: under 16 ms (docs/ARCHITECTURE.md). A fader
        // drag ends in one such call; the channel before it only wakes the audio thread.
        let mut times: Vec<Duration> = (0..20)
            .map(|i| {
                let started = Instant::now();
                audio.set_volume(&target, 0.5 + i as f32 / 100.0).expect("set_volume");
                started.elapsed()
            })
            .collect();
        times.sort();
        eprintln!("set_volume: median {:?}, slowest {:?}", times[10], times[19]);
        assert!(times[10] < Duration::from_millis(16), "median {:?}", times[10]);
        audio.set_volume(&target, 0.81).expect("set_volume");

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
    // Dropping the backend unregisters every callback before COM goes.
    drop(audio);
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
