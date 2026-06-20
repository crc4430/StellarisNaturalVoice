//! Azure Cognitive Services neural TTS backend.
//!
//! Replaces the local model: each sentence is POSTed to the Azure Speech REST
//! endpoint as SSML and the returned 24 kHz / 16-bit / mono PCM is decoded to
//! f32 samples for the SAPI write path. HTTP + TLS go through WinHTTP (the OS
//! stack), so there is no bundled model, no extra crates and no OpenSSL.

use std::ffi::c_void;

use anyhow::{anyhow, bail, Result};
use windows::core::PCWSTR;
use windows::Win32::Networking::WinHttp::*;

/// Output sample rate, fixed by the `raw-24khz-16bit-mono-pcm` output format.
pub const SAMPLE_RATE: u32 = 24000;

/// WinHTTP handle wrapper that closes on drop (handles are raw `*mut c_void`).
struct Handle(*mut c_void);
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Minimal XML escaping for text placed inside an SSML element.
fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Build the SSML body. `voice` is an Azure short name (e.g. en-US-ChristopherNeural);
/// `speed` is the engine's 0.5..2.0 multiplier mapped to an SSML relative rate.
fn build_ssml(voice: &str, text: &str, speed: f32) -> String {
    let lang = voice.split('-').take(2).collect::<Vec<_>>().join("-");
    let lang = if lang.is_empty() { "en-US".to_string() } else { lang };
    let rate_pct = (speed - 1.0) * 100.0;
    format!(
        "<speak version='1.0' xml:lang='{lang}'>\
         <voice name='{voice}'>\
         <prosody rate='{rate_pct:+.2}%'>{}</prosody>\
         </voice></speak>",
        xml_escape(text)
    )
}

/// Synthesize one chunk of text via Azure and return mono f32 samples at SAMPLE_RATE.
pub fn synthesize(
    region: &str,
    key: &str,
    voice: &str,
    text: &str,
    speed: f32,
) -> Result<Vec<f32>> {
    if region.is_empty() || key.is_empty() {
        bail!("Azure region/key not configured on the voice token");
    }
    let body = build_ssml(voice, text, speed).into_bytes();
    let pcm = http_post(region, key, &body)?;

    // Decode little-endian 16-bit PCM -> f32 in [-1, 1].
    let mut samples = Vec::with_capacity(pcm.len() / 2);
    for chunk in pcm.chunks_exact(2) {
        let v = i16::from_le_bytes([chunk[0], chunk[1]]);
        samples.push(v as f32 / 32768.0);
    }
    Ok(samples)
}

/// POST the SSML body to the Azure TTS endpoint and return the raw response bytes
/// (the PCM stream on success). Errors carry the HTTP status / body for logging.
fn http_post(region: &str, key: &str, body: &[u8]) -> Result<Vec<u8>> {
    let host = format!("{region}.tts.speech.microsoft.com");
    let headers = format!(
        "Ocp-Apim-Subscription-Key: {key}\r\n\
         Content-Type: application/ssml+xml\r\n\
         X-Microsoft-OutputFormat: raw-24khz-16bit-mono-pcm\r\n\
         User-Agent: azure-sapi"
    );

    unsafe {
        let session = WinHttpOpen(
            PCWSTR(wide("azure-sapi").as_ptr()),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        );
        if session.is_null() {
            bail!("WinHttpOpen failed");
        }
        let session = Handle(session);
        // Bound every phase so a stalled network never hangs the game thread.
        let _ = WinHttpSetTimeouts(session.0, 5000, 5000, 10000, 15000);

        let connect = WinHttpConnect(
            session.0,
            PCWSTR(wide(&host).as_ptr()),
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        );
        if connect.is_null() {
            bail!("WinHttpConnect failed");
        }
        let connect = Handle(connect);

        let request = WinHttpOpenRequest(
            connect.0,
            PCWSTR(wide("POST").as_ptr()),
            PCWSTR(wide("/cognitiveservices/v1").as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null_mut(),
            WINHTTP_FLAG_SECURE,
        );
        if request.is_null() {
            bail!("WinHttpOpenRequest failed");
        }
        let request = Handle(request);

        let hdr_w: Vec<u16> = headers.encode_utf16().collect();
        WinHttpSendRequest(
            request.0,
            Some(&hdr_w),
            Some(body.as_ptr() as *const c_void),
            body.len() as u32,
            body.len() as u32,
            0,
        )
        .map_err(|e| anyhow!("WinHttpSendRequest: {e}"))?;

        WinHttpReceiveResponse(request.0, std::ptr::null_mut())
            .map_err(|e| anyhow!("WinHttpReceiveResponse: {e}"))?;

        let status = query_status(request.0)?;
        let data = read_body(request.0)?;
        if status != 200 {
            let msg = String::from_utf8_lossy(&data);
            let snippet: String = msg.chars().take(300).collect();
            bail!("Azure HTTP {status}: {snippet}");
        }
        Ok(data)
    }
}

unsafe fn query_status(request: *mut c_void) -> Result<u32> {
    let mut code: u32 = 0;
    let mut len = std::mem::size_of::<u32>() as u32;
    let mut index = 0u32;
    WinHttpQueryHeaders(
        request,
        WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
        PCWSTR::null(),
        Some(&mut code as *mut _ as *mut c_void),
        &mut len,
        &mut index,
    )
    .map_err(|e| anyhow!("WinHttpQueryHeaders(status): {e}"))?;
    Ok(code)
}

unsafe fn read_body(request: *mut c_void) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let mut avail: u32 = 0;
        WinHttpQueryDataAvailable(request, &mut avail)
            .map_err(|e| anyhow!("WinHttpQueryDataAvailable: {e}"))?;
        if avail == 0 {
            break;
        }
        let mut buf = vec![0u8; avail as usize];
        let mut read: u32 = 0;
        WinHttpReadData(
            request,
            buf.as_mut_ptr() as *mut c_void,
            avail,
            &mut read,
        )
        .map_err(|e| anyhow!("WinHttpReadData: {e}"))?;
        if read == 0 {
            break;
        }
        buf.truncate(read as usize);
        out.extend_from_slice(&buf);
    }
    Ok(out)
}
