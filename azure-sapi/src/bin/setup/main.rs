//! Setup tool for azure-sapi: registers the voices, stores the Azure region/key
//! on the voice tokens, sets the default voice and runs a smoke test.
//!
//!   setup install
//!   setup uninstall [--purge]
//!   setup default-voice [TOKEN]
//!   setup rate <multiplier>
//!   setup test [TOKEN]
//!
//! Everything is per-user (HKCU) - no admin needed. Region/key are read from the
//! AZURE_SPEECH_REGION / AZURE_SPEECH_KEY env vars if set, otherwise prompted.

mod registry;
mod speak;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};

const DEFAULT_TOKEN: &str = "AzureThomas";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("");

    if cmd.is_empty() {
        return interactive();
    }
    let flag = |name: &str| args.iter().any(|a| a == name);
    let token_arg = || {
        args.get(1)
            .filter(|a| !a.starts_with("--"))
            .cloned()
            .unwrap_or_else(|| DEFAULT_TOKEN.to_string())
    };

    let result = match cmd {
        "install" => install(),
        "uninstall" => uninstall(flag("--purge")),
        "default-voice" => registry::set_default(&token_arg()),
        "rate" => {
            let m = args
                .get(1)
                .and_then(|a| a.parse::<f32>().ok())
                .unwrap_or(0.0);
            if m <= 0.0 {
                eprintln!("usage: setup rate <multiplier>   e.g. 1.2 = 20% faster, 1.0 = normal");
                return ExitCode::from(2);
            }
            registry::set_rate(m)
        }
        "test" => speak::speak_test(&token_arg()),
        _ => {
            eprintln!(
                "usage: setup <install | uninstall [--purge] | default-voice [TOKEN] | rate <mult> | test [TOKEN]>\n\
                 running without arguments starts the interactive install"
            );
            return ExitCode::from(2);
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn interactive() -> ExitCode {
    println!("azure-sapi setup - Azure neural TTS as a Windows voice");
    println!();
    println!("This registers the Azure voices for this user (no admin needed).");
    println!("You'll need a free Azure Speech (F0) resource: its Region (e.g. eastus)");
    println!("and one of its Keys.");
    println!();
    print!("Continue? [Y/n] ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);

    let code = match line.trim().to_lowercase().as_str() {
        "" | "y" | "yes" => match install() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::FAILURE
            }
        },
        _ => {
            println!("Cancelled.");
            ExitCode::SUCCESS
        }
    };

    println!();
    print!("Press Enter to close ...");
    let _ = std::io::stdout().flush();
    let mut discard = String::new();
    let _ = std::io::stdin().read_line(&mut discard);
    code
}

fn prompt(label: &str) -> Result<String> {
    print!("{label}: ");
    std::io::stdout().flush().ok();
    let mut s = String::new();
    std::io::stdin().read_line(&mut s).context("reading input")?;
    Ok(s.trim().to_string())
}

/// Region/key from env vars (preferred, keeps secrets off the screen) or prompt.
fn azure_credentials() -> Result<(String, String)> {
    let region = match std::env::var("AZURE_SPEECH_REGION") {
        Ok(r) if !r.trim().is_empty() => r.trim().to_string(),
        _ => prompt("Azure region (e.g. eastus)")?,
    };
    let key = match std::env::var("AZURE_SPEECH_KEY") {
        Ok(k) if !k.trim().is_empty() => k.trim().to_string(),
        _ => prompt("Azure Speech key")?,
    };
    if region.is_empty() || key.is_empty() {
        bail!("both an Azure region and key are required");
    }
    Ok((region, key))
}

fn install() -> Result<()> {
    let assets = azure_sapi::config::assets_dir(None);
    std::fs::create_dir_all(&assets).ok();

    println!("[1/4] Azure credentials ...");
    let (region, key) = azure_credentials()?;

    println!("[2/4] Installing engine DLL ...");
    let dll = install_dll(&assets)?;

    println!("[3/4] Registering voices (per-user) ...");
    registry::register_user(&dll, &assets, &region, &key)?;
    registry::set_default(DEFAULT_TOKEN)?;

    println!("[4/4] Smoke test, you should hear the voice now ...");
    speak::speak_test(DEFAULT_TOKEN)?;

    println!();
    println!("Done. Launch Stellaris and click the speaker icon on any text panel.");
    println!("Switch voice anytime: setup default-voice <AzureThomas|AzureChristopher|AzureAria|AzureGuy|AzureJenny>");
    Ok(())
}

/// Copy the engine DLL from next to setup.exe into the install dir, so the
/// registration survives `cargo clean` or deleting an extracted release zip.
fn install_dll(assets: &Path) -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let source = exe
        .parent()
        .map(|d| d.join("azure_sapi.dll"))
        .filter(|p| p.exists())
        .context("azure_sapi.dll not found next to setup.exe - build with `cargo build --release` first")?;
    let dest = assets.join("azure_sapi.dll");
    if let Err(e) = std::fs::copy(&source, &dest) {
        bail!(
            "could not copy DLL to {} ({e}); close apps using the voice (Stellaris!) and retry",
            dest.display()
        );
    }
    Ok(dest)
}

fn uninstall(purge: bool) -> Result<()> {
    registry::unregister_user()?;
    let assets = azure_sapi::config::assets_dir(None);
    if purge {
        if assets.exists() {
            std::fs::remove_dir_all(&assets)
                .with_context(|| format!("removing {}", assets.display()))?;
            println!("Removed {}", assets.display());
        }
    } else {
        println!("Install dir left in {} (use --purge to delete)", assets.display());
    }
    println!("Unregistered.");
    Ok(())
}
