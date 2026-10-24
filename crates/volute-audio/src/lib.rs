//! The sound system behind one trait (docs/ARCHITECTURE.md, decision 0002): devices, sessions,
//! volumes and peaks. The Windows backend talks to Core Audio; `MockAudio` is scripted, for
//! tests and screenshots. Nothing here knows about the UI.

use std::path::PathBuf;

mod mock;
#[cfg(windows)]
mod windows_backend;

pub use mock::MockAudio;
#[cfg(windows)]
pub use windows_backend::WindowsAudio;

/// An audio endpoint, by its Core Audio identifier.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DeviceId(pub String);

/// An audio session, by its session instance identifier: unique per session, unlike the
/// session identifier, which every session of an app shares.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SessionId(pub String);

/// The roles Windows keeps a default device for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Console,
    Multimedia,
    Communications,
}

/// A render device.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub id: DeviceId,
    /// The friendly name ("Speakers (Realtek Audio)").
    pub name: String,
    /// The endpoint scalar, 0.0 to 1.0 (Windows tapers it already).
    pub volume: f32,
    pub muted: bool,
    /// The default render device for the console role.
    pub default: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Inactive,
    Active,
    Expired,
}

/// An audio session of one process on one device.
#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    pub id: SessionId,
    pub pid: u32,
    /// The process image, when it can be read (not for system sounds, nor some elevated
    /// processes).
    pub exe: Option<PathBuf>,
    /// What the app calls its session: often empty, sometimes an indirect string ("@...").
    pub display_name: String,
    /// The session's linear scalar, 0.0 to 1.0.
    pub volume: f32,
    pub muted: bool,
    pub state: SessionState,
    pub system_sounds: bool,
}

/// What a volume or mute change applies to.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Device(DeviceId),
    Session(SessionId),
}

/// What a peak is read from: the same things a change applies to.
pub type MeterId = Target;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} not found")]
    NotFound(String),
    #[error("not supported yet")]
    Unsupported,
    #[error("{context}: {message} (0x{code:08X})")]
    Os {
        context: &'static str,
        code: u32,
        message: String,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

/// The sound system. Every call runs on the audio thread, the only one that touches Core Audio.
pub trait Audio: Send {
    /// Every render device with its volume, mute and default role.
    fn devices(&mut self, out: &mut Vec<Device>) -> Result<()>;
    /// Every audio session of a device: process, display name, volume, mute, state.
    fn sessions(&mut self, device: &DeviceId, out: &mut Vec<Session>) -> Result<()>;
    /// Peak levels (0.0 to 1.0) of the given sessions and devices, in order; 0.0 for one that
    /// is gone. Called only while a meter is shown.
    fn peaks(&mut self, ids: &[MeterId], out: &mut Vec<f32>) -> Result<()>;
    fn set_volume(&mut self, target: &Target, volume: f32) -> Result<()>;
    fn set_mute(&mut self, target: &Target, muted: bool) -> Result<()>;
    fn set_default_device(&mut self, device: &DeviceId, role: Role) -> Result<()>;
}

/// The default render device among `devices`.
pub fn default_device(devices: &[Device]) -> Option<&Device> {
    devices.iter().find(|d| d.default)
}
