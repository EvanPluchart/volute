//! Plays a quiet tone on the default render device, in an audio session of its own, until its
//! standard input closes (or 60 s pass). The owned-session test starts it, so the test only
//! ever changes the volume of a sound it made itself. Prints `playing` once the tone runs.

#[cfg(windows)]
fn main() {
    if let Err(err) = tone::play() {
        eprintln!("volute-tone: {err}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("volute-tone: Windows only");
    std::process::exit(1);
}

#[cfg(windows)]
mod tone {
    use std::io::{Read, Write};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use windows::Win32::Media::Audio::{
        AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
        AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, IAudioClient, IAudioRenderClient,
        IMMDeviceEnumerator, MMDeviceEnumerator, WAVE_FORMAT_PCM, WAVEFORMATEX, eConsole, eRender,
    };
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
    };

    const RATE: u32 = 48_000;
    const CHANNELS: u16 = 2;
    const FREQUENCY: f32 = 440.0;
    /// About -46 dBFS: audible up close, never loud.
    const AMPLITUDE: f32 = 0.005;
    const LONGEST: Duration = Duration::from_secs(60);

    pub fn play() -> windows::core::Result<()> {
        let stop = Arc::new(AtomicBool::new(false));
        let watcher = stop.clone();
        std::thread::spawn(move || {
            // Returns when the test closes our input (or dies).
            let mut sink = [0u8; 64];
            while matches!(std::io::stdin().read(&mut sink), Ok(n) if n > 0) {}
            watcher.store(true, Ordering::SeqCst);
        });

        // SAFETY: COM for this thread, then a shared-mode stream on the default device. Every
        // buffer pointer comes from `GetBuffer` and is written within the frames it granted.
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
            let block = CHANNELS * 2;
            let format = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_PCM as u16,
                nChannels: CHANNELS,
                nSamplesPerSec: RATE,
                nAvgBytesPerSec: RATE * u32::from(block),
                nBlockAlign: block,
                wBitsPerSample: 16,
                cbSize: 0,
            };
            // 200 ms of buffer, in 100 ns units; Windows converts our format to the mix format.
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                2_000_000,
                0,
                &format,
                None,
            )?;
            let size = client.GetBufferSize()?;
            let render: IAudioRenderClient = client.GetService()?;
            let mut phase = 0.0f32;
            let step = FREQUENCY / RATE as f32 * std::f32::consts::TAU;
            let mut fill = |frames: u32| -> windows::core::Result<()> {
                if frames == 0 {
                    return Ok(());
                }
                let data = render.GetBuffer(frames)? as *mut i16;
                let samples = std::slice::from_raw_parts_mut(data, (frames * u32::from(CHANNELS)) as usize);
                for frame in samples.chunks_exact_mut(usize::from(CHANNELS)) {
                    let value = (phase.sin() * AMPLITUDE * f32::from(i16::MAX)) as i16;
                    frame.fill(value);
                    phase = (phase + step) % std::f32::consts::TAU;
                }
                render.ReleaseBuffer(frames, 0)
            };
            fill(size)?;
            client.Start()?;
            println!("playing");
            let _ = std::io::stdout().flush();
            let started = Instant::now();
            while !stop.load(Ordering::SeqCst) && started.elapsed() < LONGEST {
                std::thread::sleep(Duration::from_millis(20));
                let padding = client.GetCurrentPadding()?;
                fill(size - padding)?;
            }
            client.Stop()?;
        }
        Ok(())
    }
}
