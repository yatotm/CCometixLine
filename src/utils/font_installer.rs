//! `ccline --install-font`: make Nerd Font icons work on a stock Windows setup.
//!
//! Windows ships no font with Nerd Font glyphs, so the Windows binary embeds
//! "Symbols Nerd Font Mono" (MIT, see `assets/fonts/`) and this module:
//! 1. installs it for the current user (no admin) under
//!    `%LOCALAPPDATA%\Microsoft\Windows\Fonts`,
//! 2. registers it in HKCU so it survives reboots and notifies running apps,
//! 3. drops a Windows Terminal fragment that adds it as a fallback font for the
//!    built-in Command Prompt / PowerShell profiles (Windows Terminal 1.20+).
//!
//! Afterwards ccline's own Nerd Font detection picks the font up automatically.

use std::path::{Path, PathBuf};

pub const FONT_FILE_NAME: &str = "SymbolsNerdFontMono-Regular.ttf";
pub const FONT_FAMILY: &str = "Symbols Nerd Font Mono";
/// Primary font stays Windows Terminal's default; the symbols font only fills gaps.
pub const FONT_FACE_LIST: &str = "Cascadia Mono, Symbols Nerd Font Mono";

/// Built-in Windows Terminal profiles (stable GUIDs) that receive the fallback font.
const TERMINAL_PROFILES: &[(&str, &str)] = &[
    ("Command Prompt", "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}"),
    ("Windows PowerShell", "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}"),
    ("PowerShell", "{574e775e-4f2a-5b96-ac1e-a2962a402336}"),
];

#[cfg(not(windows))]
const NOT_WINDOWS: &str =
    "--install-font is only needed on Windows; on macOS/Linux set a Nerd Font in your terminal.";

/// Per-user font folder (needs no administrator rights).
pub fn user_fonts_dir(local_app_data: &Path) -> PathBuf {
    local_app_data
        .join("Microsoft")
        .join("Windows")
        .join("Fonts")
}

/// Windows Terminal reads JSON fragments from here for both Store and unpackaged installs.
pub fn terminal_fragment_path(local_app_data: &Path) -> PathBuf {
    local_app_data
        .join("Microsoft")
        .join("Windows Terminal")
        .join("Fragments")
        .join("ccline")
        .join("nerd-font.json")
}

/// Value name under `HKCU\Software\Microsoft\Windows NT\CurrentVersion\Fonts`.
pub fn registry_value_name() -> String {
    format!("{} (TrueType)", FONT_FAMILY)
}

/// Fragment that appends the symbols font to the built-in profiles' font list.
pub fn terminal_fragment_json() -> String {
    let profiles: Vec<serde_json::Value> = TERMINAL_PROFILES
        .iter()
        .map(|(_, guid)| profile_update(guid))
        .collect();

    serde_json::to_string_pretty(&serde_json::json!({ "profiles": profiles }))
        .expect("static fragment serializes")
}

/// Only `updates` + `font` are set so user customizations (name, colors, ...) stay intact.
fn profile_update(guid: &str) -> serde_json::Value {
    serde_json::json!({
        "updates": guid,
        "font": { "face": FONT_FACE_LIST },
    })
}

#[cfg(windows)]
pub fn install() -> Result<(), Box<dyn std::error::Error>> {
    windows::install()
}

#[cfg(not(windows))]
pub fn install() -> Result<(), Box<dyn std::error::Error>> {
    Err(NOT_WINDOWS.into())
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::fs;
    use std::os::windows::ffi::OsStrExt;
    use std::process::Command;

    const FONT_BYTES: &[u8] = include_bytes!("../../assets/fonts/SymbolsNerdFontMono-Regular.ttf");
    const FONTS_REGISTRY_KEY: &str = r"HKCU\Software\Microsoft\Windows NT\CurrentVersion\Fonts";
    const HWND_BROADCAST: isize = 0xffff;
    const WM_FONTCHANGE: u32 = 0x001d;

    #[allow(non_snake_case)]
    mod ffi {
        #[link(name = "gdi32")]
        extern "system" {
            pub fn AddFontResourceW(file_name: *const u16) -> i32;
        }

        #[link(name = "user32")]
        extern "system" {
            pub fn SendNotifyMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> i32;
        }
    }

    pub fn install() -> Result<(), Box<dyn std::error::Error>> {
        let local_app_data = std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .map_err(|_| "LOCALAPPDATA is not set")?;

        let font_path = install_font_file(&local_app_data)?;
        println!("✅ Font installed: {}", font_path.display());

        register_font(&font_path)?;
        println!("✅ Registered \"{}\" for the current user", FONT_FAMILY);

        let fragment_path = write_terminal_fragment(&local_app_data)?;
        println!("✅ Windows Terminal fragment: {}", fragment_path.display());

        println!();
        println!("Restart Windows Terminal to load \"{}\".", FONT_FAMILY);
        println!("Command Prompt / PowerShell profiles now use it as a fallback font (WT 1.20+).");
        println!("Other terminals: set the font face to \"{}\".", FONT_FACE_LIST);
        println!("Undo: delete the files above and remove \"{}\" from", registry_value_name());
        println!("{}", FONTS_REGISTRY_KEY);
        Ok(())
    }

    fn install_font_file(local_app_data: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let dir = user_fonts_dir(local_app_data);
        fs::create_dir_all(&dir)?;
        let path = dir.join(FONT_FILE_NAME);
        fs::write(&path, FONT_BYTES)?;
        Ok(path)
    }

    fn register_font(font_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let value_name = registry_value_name();
        let status = Command::new("reg")
            .args(["add", FONTS_REGISTRY_KEY, "/v"])
            .arg(&value_name)
            .args(["/t", "REG_SZ", "/d"])
            .arg(font_path)
            .arg("/f")
            .status()?;
        if !status.success() {
            return Err(format!("reg add failed with {}", status).into());
        }

        // Make the font visible to running programs without signing out
        let wide: Vec<u16> = font_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives both calls,
        // and the broadcast message carries no pointers.
        unsafe {
            if ffi::AddFontResourceW(wide.as_ptr()) == 0 {
                eprintln!("⚠️  AddFontResource failed; sign out and in to load the font");
            }
            ffi::SendNotifyMessageW(HWND_BROADCAST, WM_FONTCHANGE, 0, 0);
        }
        Ok(())
    }

    fn write_terminal_fragment(
        local_app_data: &Path,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let path = terminal_fragment_path(local_app_data);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, terminal_fragment_json())?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragment_updates_each_builtin_profile_with_the_fallback_font() {
        let json = terminal_fragment_json();
        let fragment: serde_json::Value = serde_json::from_str(&json).unwrap();
        let profiles = fragment["profiles"].as_array().expect("profiles array");
        assert_eq!(profiles.len(), TERMINAL_PROFILES.len());

        for (profile, (_, guid)) in profiles.iter().zip(TERMINAL_PROFILES) {
            assert_eq!(profile["updates"], *guid);
            assert_eq!(profile["font"]["face"], FONT_FACE_LIST);
            // never override user-visible profile settings such as the name
            assert!(profile.get("name").is_none());
        }
    }

    #[test]
    fn fallback_list_keeps_the_default_font_first() {
        assert!(FONT_FACE_LIST.starts_with("Cascadia Mono, "));
        assert!(FONT_FACE_LIST.ends_with(FONT_FAMILY));
    }

    #[test]
    fn paths_follow_windows_per_user_conventions() {
        let local = Path::new("C:")
            .join("Users")
            .join("me")
            .join("AppData")
            .join("Local");

        let fonts = user_fonts_dir(&local);
        let expected_fonts = Path::new("Microsoft").join("Windows").join("Fonts");
        assert!(fonts.starts_with(&local));
        assert!(fonts.ends_with(expected_fonts));

        let fragment = terminal_fragment_path(&local);
        let expected_fragment = Path::new("Microsoft")
            .join("Windows Terminal")
            .join("Fragments")
            .join("ccline")
            .join("nerd-font.json");
        assert!(fragment.starts_with(&local));
        assert!(fragment.ends_with(expected_fragment));
    }

    #[test]
    fn registry_value_name_uses_windows_truetype_convention() {
        assert_eq!(registry_value_name(), "Symbols Nerd Font Mono (TrueType)");
    }
}
