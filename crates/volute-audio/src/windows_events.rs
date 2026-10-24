//! Core Audio's callbacks (decision 0002). They run on COM's own threads and do one thing: hand
//! a small event to the sink, which queues it for the audio thread. They never call back into
//! Core Audio, never block and never touch the UI.

use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, AudioSessionDisconnectReason, AudioSessionState,
    AudioSessionStateActive, AudioSessionStateExpired, DEVICE_STATE, EDataFlow, ERole,
    IAudioSessionControl, IAudioSessionEvents, IAudioSessionEvents_Impl,
    IAudioSessionNotification, IAudioSessionNotification_Impl, IMMNotificationClient,
    IMMNotificationClient_Impl,
};
use windows::core::{BOOL, GUID, PCWSTR, Ref, Result, implement};

use crate::windows_backend::VOLUTE_EVENT_CONTEXT;
use crate::{AudioEvent, DeviceId, EventSink, SessionId, SessionState};

/// Devices added, removed, enabled or disabled, and default changes.
#[implement(IMMNotificationClient)]
pub(crate) struct DeviceWatch {
    pub(crate) sink: EventSink,
}

impl IMMNotificationClient_Impl for DeviceWatch_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> Result<()> {
        (self.sink)(AudioEvent::DevicesChanged);
        Ok(())
    }

    fn OnDeviceAdded(&self, _: &PCWSTR) -> Result<()> {
        (self.sink)(AudioEvent::DevicesChanged);
        Ok(())
    }

    fn OnDeviceRemoved(&self, _: &PCWSTR) -> Result<()> {
        (self.sink)(AudioEvent::DevicesChanged);
        Ok(())
    }

    fn OnDefaultDeviceChanged(&self, _: EDataFlow, _: ERole, _: &PCWSTR) -> Result<()> {
        (self.sink)(AudioEvent::DevicesChanged);
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> Result<()> {
        // Frequent and rarely useful (a renamed device is read again on the next device event).
        Ok(())
    }
}

/// One device's master volume and mute.
#[implement(IAudioEndpointVolumeCallback)]
pub(crate) struct DeviceVolumeWatch {
    pub(crate) sink: EventSink,
    pub(crate) device: DeviceId,
}

impl IAudioEndpointVolumeCallback_Impl for DeviceVolumeWatch_Impl {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
        // SAFETY: Core Audio passes a valid structure for the duration of the call, or null.
        let Some(data) = (unsafe { data.as_ref() }) else {
            return Ok(());
        };
        (self.sink)(AudioEvent::DeviceVolume {
            id: self.device.clone(),
            volume: data.fMasterVolume,
            muted: data.bMuted.as_bool(),
            ours: data.guidEventContext == VOLUTE_EVENT_CONTEXT,
        });
        Ok(())
    }
}

/// New sessions on one device.
#[implement(IAudioSessionNotification)]
pub(crate) struct SessionWatch {
    pub(crate) sink: EventSink,
    pub(crate) device: DeviceId,
}

impl IAudioSessionNotification_Impl for SessionWatch_Impl {
    fn OnSessionCreated(&self, _: Ref<IAudioSessionControl>) -> Result<()> {
        (self.sink)(AudioEvent::SessionAdded {
            device: self.device.clone(),
        });
        Ok(())
    }
}

/// One session's volume, mute and state.
#[implement(IAudioSessionEvents)]
pub(crate) struct SessionEventsWatch {
    pub(crate) sink: EventSink,
    pub(crate) session: SessionId,
}

impl IAudioSessionEvents_Impl for SessionEventsWatch_Impl {
    fn OnDisplayNameChanged(&self, _: &PCWSTR, _: *const GUID) -> Result<()> {
        Ok(())
    }

    fn OnIconPathChanged(&self, _: &PCWSTR, _: *const GUID) -> Result<()> {
        Ok(())
    }

    fn OnSimpleVolumeChanged(&self, volume: f32, muted: BOOL, context: *const GUID) -> Result<()> {
        // SAFETY: a GUID valid for the call, or null.
        let ours = unsafe { context.as_ref() }.is_some_and(|g| *g == VOLUTE_EVENT_CONTEXT);
        (self.sink)(AudioEvent::SessionVolume {
            id: self.session.clone(),
            volume,
            muted: muted.as_bool(),
            ours,
        });
        Ok(())
    }

    fn OnChannelVolumeChanged(&self, _: u32, _: *const f32, _: u32, _: *const GUID) -> Result<()> {
        Ok(())
    }

    fn OnGroupingParamChanged(&self, _: *const GUID, _: *const GUID) -> Result<()> {
        Ok(())
    }

    fn OnStateChanged(&self, state: AudioSessionState) -> Result<()> {
        let state = if state == AudioSessionStateActive {
            SessionState::Active
        } else if state == AudioSessionStateExpired {
            SessionState::Expired
        } else {
            SessionState::Inactive
        };
        (self.sink)(AudioEvent::SessionState {
            id: self.session.clone(),
            state,
        });
        Ok(())
    }

    fn OnSessionDisconnected(&self, _: AudioSessionDisconnectReason) -> Result<()> {
        (self.sink)(AudioEvent::SessionState {
            id: self.session.clone(),
            state: SessionState::Expired,
        });
        Ok(())
    }
}
