//! Azure neural TTS as a SAPI5 voice.
//!
//! This cdylib is an in-process COM server implementing `ISpTTSEngine`. When a
//! SAPI host (Stellaris, System.Speech, Balabolka, ...) speaks through the
//! registered voice, each sentence is sent to the Azure Speech REST API and the
//! returned neural audio is played back. Everything is registered per-user
//! (HKCU) by the setup tool - no admin needed.

#![allow(non_snake_case)]

pub mod azure;
pub mod config;
mod engine;
mod logger;

use std::ffi::c_void;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;

use engine::AzureEngine;

/// CLSID of the engine. The setup binary derives the registry string from
/// this constant, so it is the single source of truth. (Distinct from the
/// upstream kokoro-sapi CLSID so the two can coexist.)
pub const CLSID_AZURE_ENGINE: GUID = GUID::from_u128(0x2f9b4a17_8c3d_4e62_a1f5_7d0e9b34c8a2);

/// The CLSID in registry form: {XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}.
pub fn clsid_braced() -> String {
    let g = &CLSID_AZURE_ENGINE;
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        g.data1,
        g.data2,
        g.data3,
        g.data4[0],
        g.data4[1],
        g.data4[2],
        g.data4[3],
        g.data4[4],
        g.data4[5],
        g.data4[6],
        g.data4[7]
    )
}

#[implement(IClassFactory)]
struct ClassFactory;

impl IClassFactory_Impl for ClassFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Ref<IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> Result<()> {
        if !punkouter.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let engine: IUnknown = AzureEngine::new().into();
        unsafe { engine.query(riid, ppvobject).ok() }
    }

    fn LockServer(&self, _flock: BOOL) -> Result<()> {
        Ok(())
    }
}

#[no_mangle]
extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    unsafe {
        if rclsid.is_null() || riid.is_null() || ppv.is_null() {
            return E_POINTER;
        }
        if *rclsid != CLSID_AZURE_ENGINE {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        let factory: IClassFactory = ClassFactory.into();
        factory.query(riid, ppv)
    }
}

#[no_mangle]
extern "system" fn DllCanUnloadNow() -> HRESULT {
    // The DLL stays loaded for the lifetime of the host process; SAPI hosts
    // unload it with the process.
    S_FALSE
}
