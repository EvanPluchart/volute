//! Project tasks: `cargo xtask <task>`.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

const USAGE: &str = "usage: cargo xtask check
       cargo xtask bench-overhead [--seconds=N] [--check] [--real]";
const WARM_UP: Duration = Duration::from_secs(5);

/// The performance budget from docs/ARCHITECTURE.md, per state: CPU of one core, private MB.
struct Budget {
    cpu_percent: f64,
    private_mb: f64,
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("check") => check(),
        Some("bench-overhead") => {
            let mut seconds = 60;
            let mut check = false;
            let mut real = false;
            for arg in args {
                if arg == "--check" {
                    check = true;
                    continue;
                }
                if arg == "--real" {
                    real = true;
                    continue;
                }
                match arg.strip_prefix("--seconds=").map(str::parse) {
                    Some(Ok(n)) if n > WARM_UP.as_secs() => seconds = n,
                    _ => bail!("invalid argument `{arg}`\n{USAGE}"),
                }
            }
            bench_overhead(Duration::from_secs(seconds), check, real)
        }
        _ => bail!("{USAGE}"),
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the workspace")
        .to_path_buf()
}

/// What a CI would run, run here instead (decision 0006): check, test and clippy with warnings
/// as errors, all against the committed lockfile. Stops at the first failure.
fn check() -> Result<()> {
    let root = workspace_root();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let steps: [&[&str]; 3] = [
        &["check", "--workspace", "--all-targets", "--locked"],
        &["test", "--workspace", "--locked"],
        &["clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"],
    ];
    for step in steps {
        println!("> cargo {}", step.join(" "));
        let status = Command::new(&cargo)
            .current_dir(&root)
            .args(step)
            .status()
            .context("running cargo")?;
        if !status.success() {
            bail!("`cargo {}` failed", step.join(" "));
        }
    }
    println!("check: all passed");
    Ok(())
}

/// Builds Volute in release, then runs it twice: window open with meters running, then idle in
/// the tray with nothing shown. Prints CPU of one core and peak private memory. With `check`,
/// fails when a state is over budget. On the mock, unless `real`: then on this machine's Core
/// Audio, read only (no settings file, so no remembered volume is ever applied).
fn bench_overhead(duration: Duration, check: bool, real: bool) -> Result<()> {
    let root = workspace_root();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .current_dir(&root)
        .args(["build", "--release", "--locked", "-p", "volute"])
        .status()
        .context("running cargo build")?;
    if !status.success() {
        bail!("release build failed");
    }
    let exe = root
        .join("target/release/volute")
        .with_extension(std::env::consts::EXE_EXTENSION);

    // No settings file: a benchmark neither reads nor changes the user's setup, applies no
    // remembered volume, and runs next to the user's own Volute. The mock by default.
    let source: &[&str] = if real { &[] } else { &["--mock"] };
    let states = [
        (
            "open",
            &["--no-settings", "--open", "--no-tray"][..],
            Budget {
                cpu_percent: 4.0,
                private_mb: 40.0,
            },
        ),
        (
            "idle",
            &["--no-settings"][..],
            Budget {
                cpu_percent: 0.1,
                private_mb: 25.0,
            },
        ),
    ];
    let mut over = Vec::new();
    println!();
    let on = if real { "this machine's Core Audio, read only" } else { "the mock" };
    println!("volute overhead on {on}, {} s per state", duration.as_secs());
    for (name, args, budget) in states {
        let mut child = Command::new(&exe)
            .args(source)
            .args(args)
            .spawn()
            .with_context(|| format!("starting {}", exe.display()))?;
        let result = measure(&child, duration);
        stop(&mut child);
        let m = result?;
        println!(
            "  {name:<5} CPU {:.3} % of one core (after a {} s warm-up), private {:.1} MB peak \
             ({:.1} MB at the end); budget < {} %, < {} MB",
            m.cpu_percent,
            WARM_UP.as_secs(),
            m.peak_private_mb,
            m.last_private_mb,
            budget.cpu_percent,
            budget.private_mb
        );
        if m.cpu_percent >= budget.cpu_percent || m.peak_private_mb >= budget.private_mb {
            over.push(name);
        }
    }
    if over.is_empty() {
        println!("  within budget");
    } else {
        println!("  over budget: {}", over.join(", "));
        if check {
            bail!("over budget: {}", over.join(", "));
        }
    }
    Ok(())
}

struct Measurement {
    cpu_percent: f64,
    peak_private_mb: f64,
    last_private_mb: f64,
}

fn measure(child: &Child, duration: Duration) -> Result<Measurement> {
    let start = Instant::now();
    thread::sleep(WARM_UP);
    let cpu_at_warm_up = os::cpu_time(child)?;
    let warmed = Instant::now();
    while start.elapsed() < duration {
        thread::sleep(Duration::from_millis(250));
        if os::exited(child) {
            bail!("volute exited during the benchmark");
        }
    }
    let cpu = os::cpu_time(child)?.saturating_sub(cpu_at_warm_up);
    let memory = os::memory(child)?;
    let mb = |bytes: u64| bytes as f64 / (1024.0 * 1024.0);
    Ok(Measurement {
        cpu_percent: cpu.as_secs_f64() / warmed.elapsed().as_secs_f64() * 100.0,
        peak_private_mb: mb(memory.peak_private),
        last_private_mb: mb(memory.private),
    })
}

/// Ends Volute the way the user would: closing its window, or, hidden in the tray, the
/// sign-out message its tray window quits on. Killing it would leave a dead tray icon.
fn stop(child: &mut Child) {
    let pid = child.id().to_string();
    // Without /F: a close request, as the close button sends.
    let _ = Command::new("taskkill").args(["/PID", &pid]).output();
    os::end_session(child);
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            println!("        (quit cleanly)");
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    // Started by this task, so stopping it here is safe.
    println!("        (did not quit: killed)");
    let _ = child.kill();
    let _ = child.wait();
}

struct Memory {
    private: u64,
    peak_private: u64,
}

#[cfg(windows)]
mod os {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;
    use std::time::Duration;

    use anyhow::{Result, bail};
    use windows_sys::Win32::Foundation::{FILETIME, HWND, LPARAM, WAIT_OBJECT_0};
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    };
    use windows_sys::Win32::System::Threading::{GetProcessTimes, WaitForSingleObject};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowThreadProcessId, PostMessageW, WM_ENDSESSION,
    };

    use super::Memory;

    pub fn cpu_time(child: &Child) -> Result<Duration> {
        let mut times = [FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 }; 4];
        let [created, exited, kernel, user] = &mut times;
        // SAFETY: the child's own process handle, valid while `child` lives.
        let ok = unsafe { GetProcessTimes(child.as_raw_handle(), created, exited, kernel, user) };
        if ok == 0 {
            bail!("reading volute's CPU time");
        }
        let ticks = |t: &FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
        // 100 ns units.
        Ok(Duration::from_nanos((ticks(kernel) + ticks(user)) * 100))
    }

    pub fn memory(child: &Child) -> Result<Memory> {
        let mut counters = PROCESS_MEMORY_COUNTERS_EX {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        // SAFETY: as above; the structure is sized for the extended counters.
        let ok = unsafe {
            GetProcessMemoryInfo(
                child.as_raw_handle(),
                (&raw mut counters).cast::<PROCESS_MEMORY_COUNTERS>(),
                counters.cb,
            )
        };
        if ok == 0 {
            bail!("reading volute's memory");
        }
        Ok(Memory {
            private: counters.PrivateUsage as u64,
            // The commit charge's peak: private memory at its highest over the whole run.
            peak_private: counters.PeakPagefileUsage as u64,
        })
    }

    pub fn exited(child: &Child) -> bool {
        // SAFETY: as above; a zero timeout only polls.
        unsafe { WaitForSingleObject(child.as_raw_handle(), 0) == WAIT_OBJECT_0 }
    }

    /// Posts "the session ends" to the child's tray window, which quits on it (decision 0004).
    pub fn end_session(child: &Child) {
        struct Search {
            pid: u32,
            found: HWND,
        }
        unsafe extern "system" fn each(hwnd: HWND, data: LPARAM) -> i32 {
            // SAFETY: `data` points to the `Search` below, alive during EnumWindows.
            let search = unsafe { &mut *(data as *mut Search) };
            let mut pid = 0;
            let mut class = [0u16; 64];
            // SAFETY: plain queries into buffers that outlive the calls.
            let len = unsafe {
                GetWindowThreadProcessId(hwnd, &mut pid);
                GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32)
            };
            if pid == search.pid && String::from_utf16_lossy(&class[..len.max(0) as usize]) == "VoluteTrayWindow" {
                search.found = hwnd;
                return 0;
            }
            1
        }
        let mut search = Search {
            pid: child.id(),
            found: std::ptr::null_mut(),
        };
        // SAFETY: the callback only reads windows and writes `search`.
        unsafe {
            EnumWindows(Some(each), (&raw mut search) as LPARAM);
            if !search.found.is_null() {
                PostMessageW(search.found, WM_ENDSESSION, 1, 0);
            }
        }
    }
}

#[cfg(not(windows))]
mod os {
    use std::process::Child;
    use std::time::Duration;

    use anyhow::{Result, bail};

    use super::Memory;

    pub fn cpu_time(_: &Child) -> Result<Duration> {
        bail!("bench-overhead runs on Windows")
    }

    pub fn memory(_: &Child) -> Result<Memory> {
        bail!("bench-overhead runs on Windows")
    }

    pub fn exited(_: &Child) -> bool {
        true
    }

    pub fn end_session(_: &Child) {}
}
