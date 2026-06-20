//! Registry plumbing: COM class, SAPI voice tokens, default voice. Everything is
//! per-user (HKCU) - no admin needed. The Azure region + key are stored on each
//! voice token; the engine DLL reads them back via the token.

use std::path::Path;

use anyhow::{bail, Context, Result};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

pub struct VoiceDef {
    pub token: &'static str,
    pub display: &'static str,
    /// Azure neural voice short name.
    pub voice_name: &'static str,
    pub gender: &'static str,
    /// LANGID hex string: 409 = en-US, 809 = en-GB.
    pub language: &'static str,
}

pub const VOICES: [VoiceDef; 5] = [
    VoiceDef { token: "AzureChristopher", display: "Azure Christopher (en-US)", voice_name: "en-US-ChristopherNeural", gender: "Male", language: "409" },
    VoiceDef { token: "AzureThomas", display: "Azure Thomas (en-GB)", voice_name: "en-GB-ThomasNeural", gender: "Male", language: "809" },
    VoiceDef { token: "AzureAria", display: "Azure Aria (en-US)", voice_name: "en-US-AriaNeural", gender: "Female", language: "409" },
    VoiceDef { token: "AzureGuy", display: "Azure Guy (en-US)", voice_name: "en-US-GuyNeural", gender: "Male", language: "409" },
    VoiceDef { token: "AzureJenny", display: "Azure Jenny (en-US)", voice_name: "en-US-JennyNeural", gender: "Female", language: "409" },
];

const SAPI_TOKENS: &str = r"SOFTWARE\Microsoft\Speech\Voices\Tokens";
const SPEECH_VOICES: &str = r"SOFTWARE\Microsoft\Speech\Voices";
/// The "modern" OneCore default voice. Stellaris and other OneCore apps read
/// THIS, not the classic DefaultTokenId.
const ONECORE_TTS: &str = r"SOFTWARE\Microsoft\Speech_OneCore\Settings\TextToSpeech";

pub fn register_user(dll: &Path, assets: &Path, region: &str, key_value: &str) -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let clsid = azure_sapi::clsid_braced();
    let (inproc, _) = hkcu
        .create_subkey(format!(r"Software\Classes\CLSID\{clsid}\InprocServer32"))
        .context("creating COM class key")?;
    inproc.set_value("", &dll.to_string_lossy().to_string())?;
    inproc.set_value("ThreadingModel", &"Both")?;

    for v in &VOICES {
        let (key, _) = hkcu
            .create_subkey(format!(r"{SAPI_TOKENS}\{}", v.token))
            .with_context(|| format!("creating token {}", v.token))?;
        key.set_value("", &v.display)?;
        key.set_value("CLSID", &clsid)?;
        key.set_value("VoiceName", &v.voice_name)?;
        key.set_value("Region", &region)?;
        key.set_value("ApiKey", &key_value)?;
        key.set_value("AssetsDir", &assets.to_string_lossy().to_string())?;
        // Preserve a user-set speed across re-installs; default 1.0 (= as asked).
        let existing_rate: String = key.get_value("RateBoost").unwrap_or_default();
        let rate = if existing_rate.is_empty() { "1.0".to_string() } else { existing_rate };
        key.set_value("RateBoost", &rate)?;
        let (attrs, _) = key.create_subkey("Attributes")?;
        attrs.set_value("Name", &v.display)?;
        attrs.set_value("Gender", &v.gender)?;
        attrs.set_value("Age", &"Adult")?;
        attrs.set_value("Vendor", &"Azure")?;
        attrs.set_value("Language", &v.language)?;
        println!("  voice: {}", v.display);
    }
    Ok(())
}

pub fn unregister_user() -> Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let clsid = azure_sapi::clsid_braced();
    let _ = hkcu.delete_subkey_all(format!(r"Software\Classes\CLSID\{clsid}"));
    for v in &VOICES {
        let _ = hkcu.delete_subkey_all(format!(r"{SAPI_TOKENS}\{}", v.token));
    }
    // Clear the classic default if it points at one of our voices.
    if let Ok(key) = hkcu.open_subkey_with_flags(SPEECH_VOICES, winreg::enums::KEY_ALL_ACCESS) {
        let default: String = key.get_value("DefaultTokenId").unwrap_or_default();
        if default.contains(r"\Tokens\Azure") {
            let _ = key.delete_value("DefaultTokenId");
        }
    }
    Ok(())
}

/// Sets the per-user classic-SAPI default voice and clears the modern OneCore
/// default-voice override so OneCore apps (Stellaris) fall back to it.
pub fn set_default(token: &str) -> Result<()> {
    if !VOICES.iter().any(|v| v.token == token) {
        bail!(
            "unknown voice token {token:?}; available: {}",
            VOICES.map(|v| v.token).join(", ")
        );
    }
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    hkcu.open_subkey(format!(r"{SAPI_TOKENS}\{token}"))
        .context("voice is not registered - run `setup install` first")?;

    let (key, _) = hkcu.create_subkey(SPEECH_VOICES)?;
    key.set_value(
        "DefaultTokenId",
        &format!(r"HKEY_CURRENT_USER\{SAPI_TOKENS}\{token}"),
    )?;
    println!("  default classic-SAPI voice: {token}");

    // CRITICAL for Stellaris (and other OneCore apps): they read this "modern"
    // default-voice value, NOT the classic DefaultTokenId above. Settings UIs
    // (sapi.cpl, Win+I -> Speech) write it to a Microsoft voice, which then
    // overrides our default and plays a robotic voice. Clearing it makes those
    // apps fall back to the classic SAPI default = our voice.
    if let Ok(tts) = hkcu.open_subkey_with_flags(ONECORE_TTS, winreg::enums::KEY_ALL_ACCESS) {
        if tts.delete_value("Voice").is_ok() {
            println!("  cleared OneCore TextToSpeech override (so Stellaris uses {token})");
        }
    }
    Ok(())
}

/// Sets the speed multiplier on every registered voice token. 1.0 = as the host
/// asks; 1.2 = 20% faster. Clamped to 0.5..2.0.
pub fn set_rate(multiplier: f32) -> Result<()> {
    let m = multiplier.clamp(0.5, 2.0);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let mut wrote = 0u32;
    for v in &VOICES {
        if let Ok(key) = hkcu.open_subkey_with_flags(
            format!(r"{SAPI_TOKENS}\{}", v.token),
            winreg::enums::KEY_SET_VALUE,
        ) {
            if key.set_value("RateBoost", &format!("{m}")).is_ok() {
                wrote += 1;
            }
        }
    }
    if wrote == 0 {
        bail!("no registered voices found - run `setup install` first");
    }
    println!("  speed set to {m}x on all voices");
    println!("  (restart Stellaris for it to take effect)");
    Ok(())
}
