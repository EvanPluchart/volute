//! Closing the window ends the process (decision 0004). A Volute that stays alive unseen holds
//! the single instance, and no later launch opens it again.

/// Starts Volute on the mock with `args`, asks its windows to close as the close button does,
/// and checks the process ends.
#[cfg(windows)]
fn closes_with(args: &[&str]) {
    use std::process::Command;
    use std::time::{Duration, Instant};

    // The test starts this Volute itself, so closing it here is safe. The mock, no settings:
    // no real audio is touched and the user's own Volute is left alone.
    let mut volute = Command::new(env!("CARGO_BIN_EXE_volute"))
        .args(["--mock", "--no-settings"])
        .args(args)
        .spawn()
        .expect("volute starts");
    std::thread::sleep(Duration::from_secs(3));
    assert!(
        volute.try_wait().expect("status").is_none(),
        "volute is running"
    );

    // Without /F, taskkill asks the windows to close.
    let status = Command::new("taskkill")
        .args(["/PID", &volute.id().to_string()])
        .output()
        .expect("taskkill runs");
    assert!(status.status.success(), "the close request was delivered");

    let asked = Instant::now();
    let deadline = asked + Duration::from_secs(8);
    while Instant::now() < deadline {
        if volute.try_wait().expect("status").is_some() {
            // The exit watchdog fires after 3 s: ending sooner means every thread quit.
            assert!(
                asked.elapsed() < Duration::from_millis(2500),
                "volute ({args:?}) ended only after {:?}: the watchdog, not a clean quit",
                asked.elapsed()
            );
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = volute.kill();
    panic!("volute ({args:?}) was still running 8 s after its window closed");
}

#[cfg(windows)]
#[test]
fn closing_the_window_ends_the_process() {
    closes_with(&["--open", "--no-tray"]);
}

/// With its tray icon, whose hidden window ignores other programs' WM_CLOSE: closing the main
/// window still ends Volute, and the icon goes with it.
#[cfg(windows)]
#[test]
fn closing_the_window_ends_the_process_with_a_tray_icon() {
    closes_with(&["--open"]);
}
