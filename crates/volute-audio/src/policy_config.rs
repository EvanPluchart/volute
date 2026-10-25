//! `IPolicyConfig`, the undocumented interface that sets the default audio device (decision
//! 0002). Every third-party mixer uses it. Its identifiers and method order are taken from
//! EarTrumpet (File-New-Project/EarTrumpet, EarTrumpet/Interop/MMDeviceAPI/IPolicyConfig.cs and
//! PolicyConfigClient.cs, commit 00770d32): the same interface from Windows 7 to today, except
//! Windows 10 1507 and 1511, where its identifier differed. Anything failing reports
//! "unsupported" instead of breaking the app.
//!
//! Never called by a test: it changes the user's machine.

use std::ffi::c_void;

use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{ERole, eCommunications, eConsole, eMultimedia};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::core::{GUID, HRESULT, IUnknown, IUnknown_Vtbl, PCWSTR, interface};

use crate::{DeviceId, Error, Result, Role};

/// CLSID_PolicyConfigClient.
const POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

/// Only `set_default_endpoint` (SetDefaultEndpoint) is called; the slots before it keep the
/// vtable's order, so their names do not matter.
#[interface("F8679F50-850A-41CF-9C72-430F290290C8")]
unsafe trait IPolicyConfig: IUnknown {
    fn unused_1(&self) -> HRESULT;
    fn unused_2(&self) -> HRESULT;
    fn unused_3(&self) -> HRESULT;
    fn unused_4(&self) -> HRESULT;
    fn unused_5(&self) -> HRESULT;
    fn unused_6(&self) -> HRESULT;
    fn unused_7(&self) -> HRESULT;
    fn unused_8(&self) -> HRESULT;
    fn get_property_value(&self, device: PCWSTR, key: *const PROPERTYKEY, value: *mut c_void) -> HRESULT;
    fn set_property_value(&self, device: PCWSTR, key: *const PROPERTYKEY, value: *mut c_void) -> HRESULT;
    fn set_default_endpoint(&self, device: PCWSTR, role: ERole) -> HRESULT;
    fn set_endpoint_visibility(&self, device: PCWSTR, visible: i16) -> HRESULT;
}

/// Makes `device` the default for `role`. COM must be initialised on this thread.
pub(crate) fn set_default(device: &DeviceId, role: Role) -> Result<()> {
    let role = match role {
        Role::Console => eConsole,
        Role::Multimedia => eMultimedia,
        Role::Communications => eCommunications,
    };
    let id: Vec<u16> = device.0.encode_utf16().chain([0]).collect();
    // SAFETY: creating the policy client and calling its eleventh method with a NUL-terminated
    // identifier that outlives the call.
    unsafe {
        let policy: IPolicyConfig =
            CoCreateInstance(&POLICY_CONFIG_CLIENT, None, CLSCTX_ALL).map_err(|_| Error::Unsupported)?;
        policy
            .set_default_endpoint(PCWSTR(id.as_ptr()), role)
            .ok()
            .map_err(|e| Error::Os {
                context: "setting the default device",
                code: e.code().0 as u32,
                message: e.message().trim().to_owned(),
            })
    }
}
