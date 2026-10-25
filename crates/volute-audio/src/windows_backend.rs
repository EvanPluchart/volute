//! Core Audio (decision 0002). Created and used on the audio thread only, in the multithreaded
//! apartment. Changes arrive as events (windows_events.rs); peaks are read on demand.

use std::collections::HashMap;
use std::path::PathBuf;

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{CloseHandle, S_OK};
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioMeterInformation,
};
use windows::Win32::Media::Audio::{
    AudioSessionStateActive, AudioSessionStateExpired, DEVICE_STATE_ACTIVE, IAudioSessionControl2,
    IAudioSessionEvents, IAudioSessionManager2, IAudioSessionNotification, IMMDevice,
    IMMDeviceEnumerator, IMMEndpoint, IMMNotificationClient, ISimpleAudioVolume,
    MMDeviceEnumerator, eAll, eCapture, eConsole, eRender,
};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize, STGM_READ,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows::core::{GUID, Interface, PWSTR};

use crate::windows_events::{DeviceVolumeWatch, DeviceWatch, SessionEventsWatch, SessionWatch};
use crate::{
    Audio, Device, DeviceId, Error, EventSink, Flow, MeterId, Result, Role, Session, SessionId,
    SessionState, Target,
};

/// The event context of every change Volute makes, so its own echo can be told from another
/// program's change.
pub const VOLUTE_EVENT_CONTEXT: GUID = GUID::from_u128(0x5f0b_7c2e_93d4_4a61_b8e5_0c6d_2a91_f3b7);

fn os(context: &'static str) -> impl FnOnce(windows::core::Error) -> Error {
    move |e| Error::Os {
        context,
        code: e.code().0 as u32,
        message: e.message().trim().to_owned(),
    }
}

/// Takes a string Core Audio allocated, and frees it.
///
/// # Safety
/// `text` must be null or a NUL-terminated string allocated with `CoTaskMemAlloc`.
unsafe fn take_string(text: PWSTR) -> String {
    if text.is_null() {
        return String::new();
    }
    // SAFETY: per the contract, a valid string, freed once here.
    unsafe {
        let value = text.to_string().unwrap_or_default();
        CoTaskMemFree(Some(text.as_ptr() as *const _));
        value
    }
}

struct DeviceEntry {
    device: IMMDevice,
    flow: Flow,
    volume: IAudioEndpointVolume,
    meter: IAudioMeterInformation,
    name: String,
    /// Registered for volume changes while listening.
    volume_watch: Option<IAudioEndpointVolumeCallback>,
    /// The session manager and its new-session watch, once the device's sessions were read.
    sessions: Option<(IAudioSessionManager2, Option<IAudioSessionNotification>)>,
}

impl DeviceEntry {
    /// Stops every notification of this device. Never called from a callback.
    fn unregister(&mut self) {
        // SAFETY: unregistering callbacks this thread registered on objects it owns.
        unsafe {
            if let Some(watch) = self.volume_watch.take() {
                let _ = self.volume.UnregisterControlChangeNotify(&watch);
            }
            if let Some((manager, Some(watch))) = self.sessions.as_mut().map(|(m, w)| (m, w.take())) {
                let _ = manager.UnregisterSessionNotification(&watch);
            }
        }
    }
}

struct SessionEntry {
    device: DeviceId,
    control: IAudioSessionControl2,
    volume: ISimpleAudioVolume,
    meter: IAudioMeterInformation,
    pid: u32,
    exe: Option<PathBuf>,
    system_sounds: bool,
    events: Option<IAudioSessionEvents>,
    /// Seen in the last enumeration of its device.
    seen: bool,
}

impl SessionEntry {
    fn unregister(&mut self) {
        if let Some(events) = self.events.take() {
            // SAFETY: unregistering a callback this thread registered.
            let _ = unsafe { self.control.UnregisterAudioSessionNotification(&events) };
        }
    }
}

/// Ends COM on this thread when the backend goes, after every interface it holds.
struct Com;

impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: pairs the successful CoInitializeEx in `WindowsAudio::new`.
        unsafe { CoUninitialize() };
    }
}

pub struct WindowsAudio {
    enumerator: IMMDeviceEnumerator,
    sink: Option<EventSink>,
    device_watch: Option<IMMNotificationClient>,
    devices: HashMap<String, DeviceEntry>,
    sessions: HashMap<String, SessionEntry>,
    // Last field: dropped after the interfaces above.
    _com: Com,
}

// SAFETY: every interface is created in the multithreaded apartment, where COM pointers may be
// used from any thread of that apartment. The backend is created on the audio thread and stays
// there; `Send` only lets the app hand it a constructor's result.
unsafe impl Send for WindowsAudio {}

impl Drop for WindowsAudio {
    fn drop(&mut self) {
        // Every callback is unregistered here, on the audio thread, before the objects it points
        // to and COM itself go: a callback must never land in freed memory at exit.
        for session in self.sessions.values_mut() {
            session.unregister();
        }
        for device in self.devices.values_mut() {
            device.unregister();
        }
        if let Some(watch) = self.device_watch.take() {
            // SAFETY: unregistering the callback `listen` registered.
            let _ = unsafe { self.enumerator.UnregisterEndpointNotificationCallback(&watch) };
        }
    }
}

impl WindowsAudio {
    /// Joins the multithreaded apartment on the calling thread, which must be the audio thread
    /// for the backend's whole life.
    pub fn new() -> Result<WindowsAudio> {
        // SAFETY: initialising COM for this thread; undone by `Com` on drop.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(os("joining COM"))?;
        let com = Com;
        // SAFETY: creating the system's device enumerator.
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
                .map_err(os("creating the device enumerator"))?;
        Ok(WindowsAudio {
            enumerator,
            sink: None,
            device_watch: None,
            devices: HashMap::new(),
            sessions: HashMap::new(),
            _com: com,
        })
    }

    /// The device entry for `id`, opened and cached on first use.
    fn device(&mut self, id: &str) -> Result<&mut DeviceEntry> {
        if !self.devices.contains_key(id) {
            let wide: Vec<u16> = id.encode_utf16().chain([0]).collect();
            // SAFETY: a NUL-terminated identifier that outlives the call.
            let device = unsafe { self.enumerator.GetDevice(windows::core::PCWSTR(wide.as_ptr())) }
                .map_err(|_| Error::NotFound(format!("device {id}")))?;
            let entry = open_device(device, id, self.sink.as_ref())?;
            self.devices.insert(id.to_owned(), entry);
        }
        Ok(self.devices.get_mut(id).expect("inserted above"))
    }

    fn session(&self, id: &SessionId) -> Result<&SessionEntry> {
        self.sessions
            .get(&id.0)
            .ok_or_else(|| Error::NotFound(format!("session {}", id.0)))
    }
}

fn open_device(device: IMMDevice, id: &str, sink: Option<&EventSink>) -> Result<DeviceEntry> {
    // SAFETY: activating interfaces of a live device; the property variant is cleared after
    // its string is copied; the callback is unregistered by `DeviceEntry::unregister`.
    unsafe {
        let volume: IAudioEndpointVolume = device
            .Activate(CLSCTX_ALL, None)
            .map_err(os("opening a device's volume"))?;
        let meter: IAudioMeterInformation = device
            .Activate(CLSCTX_ALL, None)
            .map_err(os("opening a device's meter"))?;
        let flow = match device.cast::<IMMEndpoint>().and_then(|e| e.GetDataFlow()) {
            Ok(f) if f == eCapture => Flow::Capture,
            _ => Flow::Render,
        };
        let mut name = String::new();
        if let Ok(store) = device.OpenPropertyStore(STGM_READ)
            && let Ok(mut value) = store.GetValue(&PKEY_Device_FriendlyName)
        {
            if let Ok(text) = PropVariantToStringAlloc(&value) {
                name = take_string(text);
            }
            let _ = PropVariantClear(&mut value);
        }
        let volume_watch = sink.and_then(|sink| {
            let watch: IAudioEndpointVolumeCallback = DeviceVolumeWatch {
                sink: sink.clone(),
                device: DeviceId(id.to_owned()),
            }
            .into();
            volume.RegisterControlChangeNotify(&watch).ok().map(|()| watch)
        });
        Ok(DeviceEntry {
            device,
            flow,
            volume,
            meter,
            name,
            volume_watch,
            sessions: None,
        })
    }
}

/// The image path of process `pid`, read with limited query rights (decision 0005).
fn process_image(pid: u32) -> Option<PathBuf> {
    if pid == 0 {
        return None;
    }
    // SAFETY: a handle with limited query rights, closed before returning; the buffer outlives
    // the call and its length is passed.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = vec![0u16; 1024];
        let mut len = buffer.len() as u32;
        let read = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(process);
        read.ok()?;
        Some(PathBuf::from(String::from_utf16_lossy(&buffer[..len as usize])))
    }
}

impl Audio for WindowsAudio {
    fn listen(&mut self, sink: EventSink) -> Result<()> {
        let watch: IMMNotificationClient = DeviceWatch { sink: sink.clone() }.into();
        // SAFETY: registering a callback object that lives in `device_watch` until `drop`.
        unsafe { self.enumerator.RegisterEndpointNotificationCallback(&watch) }
            .map_err(os("watching devices"))?;
        self.device_watch = Some(watch);
        self.sink = Some(sink);
        Ok(())
    }

    fn devices(&mut self, out: &mut Vec<Device>) -> Result<()> {
        out.clear();
        // SAFETY: plain Core Audio queries on interfaces this thread owns; strings are freed by
        // `take_string`.
        unsafe {
            let default_of = |flow| {
                self.enumerator
                    .GetDefaultAudioEndpoint(flow, eConsole)
                    .ok()
                    .and_then(|d| d.GetId().ok())
                    .map(|id| take_string(id))
            };
            let defaults = [default_of(eRender), default_of(eCapture)];
            let collection = self
                .enumerator
                .EnumAudioEndpoints(eAll, DEVICE_STATE_ACTIVE)
                .map_err(os("listing devices"))?;
            let count = collection.GetCount().map_err(os("counting devices"))?;
            let mut live = Vec::with_capacity(count as usize);
            for i in 0..count {
                let Ok(device) = collection.Item(i) else { continue };
                let Ok(id) = device.GetId() else { continue };
                let id = take_string(id);
                if !self.devices.contains_key(&id) {
                    let Ok(entry) = open_device(device, &id, self.sink.as_ref()) else { continue };
                    self.devices.insert(id.clone(), entry);
                }
                let entry = &self.devices[&id];
                out.push(Device {
                    id: DeviceId(id.clone()),
                    name: entry.name.clone(),
                    flow: entry.flow,
                    volume: entry.volume.GetMasterVolumeLevelScalar().unwrap_or(0.0),
                    muted: entry.volume.GetMute().is_ok_and(|m| m.as_bool()),
                    default: defaults.iter().any(|d| d.as_deref() == Some(id.as_str())),
                });
                live.push(id);
            }
            // Devices unplugged since: stop their notifications and forget them, and their
            // sessions.
            for (_, mut gone) in self.devices.extract_if(|id, _| !live.contains(id)) {
                gone.unregister();
            }
            for (_, mut gone) in self.sessions.extract_if(|_, s| !live.contains(&s.device.0)) {
                gone.unregister();
            }
        }
        Ok(())
    }

    fn sessions(&mut self, device: &DeviceId, out: &mut Vec<Session>) -> Result<()> {
        out.clear();
        let sink = self.sink.clone();
        let manager: IAudioSessionManager2 = {
            let entry = self.device(&device.0)?;
            if entry.sessions.is_none() {
                // SAFETY: activating the session manager of a live device, and registering a
                // watch unregistered by `DeviceEntry::unregister`.
                let manager: IAudioSessionManager2 = unsafe { entry.device.Activate(CLSCTX_ALL, None) }
                    .map_err(os("opening the session manager"))?;
                let watch = sink.as_ref().and_then(|sink| {
                    let watch: IAudioSessionNotification = SessionWatch {
                        sink: sink.clone(),
                        device: device.clone(),
                    }
                    .into();
                    // SAFETY: as above.
                    unsafe { manager.RegisterSessionNotification(&watch) }.ok().map(|()| watch)
                });
                entry.sessions = Some((manager, watch));
            }
            entry.sessions.as_ref().expect("set above").0.clone()
        };
        for entry in self.sessions.values_mut() {
            if entry.device == *device {
                entry.seen = false;
            }
        }
        // SAFETY: enumerating sessions on interfaces this thread owns (which also arms the
        // new-session notifications); strings are freed by `take_string`; session callbacks are
        // unregistered by `SessionEntry::unregister`.
        unsafe {
            let sessions = manager
                .GetSessionEnumerator()
                .map_err(os("listing sessions"))?;
            let count = sessions.GetCount().map_err(os("counting sessions"))?;
            for i in 0..count {
                let Ok(control) = sessions.GetSession(i) else { continue };
                let Ok(control) = control.cast::<IAudioSessionControl2>() else { continue };
                let state = match control.GetState() {
                    Ok(s) if s == AudioSessionStateActive => SessionState::Active,
                    Ok(s) if s == AudioSessionStateExpired => continue,
                    Ok(_) => SessionState::Inactive,
                    Err(_) => continue,
                };
                let Ok(instance) = control.GetSessionInstanceIdentifier() else { continue };
                let instance = take_string(instance);
                if !self.sessions.contains_key(&instance) {
                    let (Ok(volume), Ok(meter)) = (
                        control.cast::<ISimpleAudioVolume>(),
                        control.cast::<IAudioMeterInformation>(),
                    ) else {
                        continue;
                    };
                    let pid = control.GetProcessId().unwrap_or(0);
                    let events = sink.as_ref().and_then(|sink| {
                        let events: IAudioSessionEvents = SessionEventsWatch {
                            sink: sink.clone(),
                            session: SessionId(instance.clone()),
                        }
                        .into();
                        control.RegisterAudioSessionNotification(&events).ok().map(|()| events)
                    });
                    self.sessions.insert(
                        instance.clone(),
                        SessionEntry {
                            device: device.clone(),
                            system_sounds: control.IsSystemSoundsSession() == S_OK,
                            exe: process_image(pid),
                            pid,
                            control,
                            volume,
                            meter,
                            events,
                            seen: true,
                        },
                    );
                }
                let entry = self.sessions.get_mut(&instance).expect("inserted above");
                entry.seen = true;
                out.push(Session {
                    id: SessionId(instance),
                    pid: entry.pid,
                    exe: entry.exe.clone(),
                    display_name: entry
                        .control
                        .GetDisplayName()
                        .map(|n| take_string(n))
                        .unwrap_or_default(),
                    volume: entry.volume.GetMasterVolume().unwrap_or(0.0),
                    muted: entry.volume.GetMute().is_ok_and(|m| m.as_bool()),
                    state,
                    system_sounds: entry.system_sounds,
                });
            }
        }
        // Sessions that expired or vanished: stop their notifications, drop their interfaces.
        for (_, mut gone) in self.sessions.extract_if(|_, s| s.device == *device && !s.seen) {
            gone.unregister();
        }
        Ok(())
    }

    fn peaks(&mut self, ids: &[MeterId], out: &mut Vec<f32>) -> Result<()> {
        out.clear();
        for id in ids {
            let meter = match id {
                Target::Session(s) => self.sessions.get(&s.0).map(|e| &e.meter),
                Target::Device(d) => self.devices.get(&d.0).map(|e| &e.meter),
            };
            // SAFETY: reading a meter this thread owns.
            let peak = meter.map_or(0.0, |m| unsafe { m.GetPeakValue() }.unwrap_or(0.0));
            out.push(peak.clamp(0.0, 1.0));
        }
        Ok(())
    }

    fn set_volume(&mut self, target: &Target, volume: f32) -> Result<()> {
        let volume = volume.clamp(0.0, 1.0);
        match target {
            Target::Session(id) => {
                let entry = self.session(id)?;
                // SAFETY: a session volume this thread owns; the GUID outlives the call.
                unsafe { entry.volume.SetMasterVolume(volume, &VOLUTE_EVENT_CONTEXT) }
                    .map_err(os("setting a session's volume"))
            }
            Target::Device(id) => {
                let entry = self.device(&id.0)?;
                // SAFETY: as above, for a device.
                unsafe {
                    entry
                        .volume
                        .SetMasterVolumeLevelScalar(volume, &VOLUTE_EVENT_CONTEXT)
                }
                .map_err(os("setting a device's volume"))
            }
        }
    }

    fn set_mute(&mut self, target: &Target, muted: bool) -> Result<()> {
        match target {
            Target::Session(id) => {
                let entry = self.session(id)?;
                // SAFETY: a session volume this thread owns; the GUID outlives the call.
                unsafe { entry.volume.SetMute(muted, &VOLUTE_EVENT_CONTEXT) }
                    .map_err(os("muting a session"))
            }
            Target::Device(id) => {
                let entry = self.device(&id.0)?;
                // SAFETY: as above, for a device.
                unsafe { entry.volume.SetMute(muted, &VOLUTE_EVENT_CONTEXT) }
                    .map_err(os("muting a device"))
            }
        }
    }

    fn set_default_device(&mut self, device: &DeviceId, role: Role) -> Result<()> {
        crate::policy_config::set_default(device, role)
    }
}
