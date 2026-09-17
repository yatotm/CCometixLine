//! Platform icon compatibility.
//!
//! Nerd Font icons live in Unicode Private Use Areas and only render when the
//! terminal font is a Nerd Font. Two fallbacks are applied at render time:
//!
//! * No Nerd Font installed (detected on Windows by scanning the font folders,
//!   or forced with `CCLINE_NERD_FONT=0`): segment icons use their `plain`
//!   (emoji) variant and any remaining PUA glyph becomes a plain Unicode symbol.
//! * Nerd Font present on Windows: Material Design glyphs (U+F0000+, which
//!   need surrogate pairs and often show as `?` in Windows consoles) are
//!   swapped for BMP equivalents from Font Awesome / Devicons.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// `CCLINE_NERD_FONT=1` forces Nerd Font icons, `0` forces the plain fallback.
const ENV_OVERRIDE: &str = "CCLINE_NERD_FONT";

/// Material Design icon → BMP Nerd Font equivalent (Windows with a Nerd Font).
const BMP_FALLBACKS: &[(char, char)] = &[
    ('\u{f024b}', '\u{f07b}'), // folder            → fa-folder
    ('\u{f02a2}', '\u{e725}'), // git               → dev-git_branch
    ('\u{f19bb}', '\u{f017}'), // session timer     → fa-clock_o
    ('\u{f1ad3}', '\u{f017}'), // session timer     → fa-clock_o
    ('\u{f12f5}', '\u{f05b}'), // output style      → fa-crosshairs
    ('\u{f06b0}', '\u{f021}'), // update available  → fa-refresh
    ('\u{f0a9e}', '\u{f10c}'), // circle_slice_1    → fa-circle_o
    ('\u{f0a9f}', '\u{f10c}'), // circle_slice_2    → fa-circle_o
    ('\u{f0aa0}', '\u{f042}'), // circle_slice_3    → fa-adjust
    ('\u{f0aa1}', '\u{f042}'), // circle_slice_4    → fa-adjust
    ('\u{f0aa2}', '\u{f042}'), // circle_slice_5    → fa-adjust
    ('\u{f0aa3}', '\u{f042}'), // circle_slice_6    → fa-adjust
    ('\u{f0aa4}', '\u{f111}'), // circle_slice_7    → fa-circle
    ('\u{f0aa5}', '\u{f111}'), // circle_slice_8    → fa-circle
];

/// Used for supplementary-PUA icons without a dedicated BMP mapping.
const GENERIC_BMP_FALLBACK: char = '\u{f111}'; // fa-circle

/// Nerd Font glyph → plain Unicode symbol (no Nerd Font available).
/// Segment icons are handled by their `plain` variant; this covers glyphs that
/// are generated at runtime (usage dial, update notice, powerline separator).
const PLAIN_FALLBACKS: &[(char, char)] = &[
    ('\u{f0a9e}', '○'), // circle_slice_1
    ('\u{f0a9f}', '◔'), // circle_slice_2
    ('\u{f0aa0}', '◔'), // circle_slice_3
    ('\u{f0aa1}', '◑'), // circle_slice_4
    ('\u{f0aa2}', '◑'), // circle_slice_5
    ('\u{f0aa3}', '◕'), // circle_slice_6
    ('\u{f0aa4}', '◕'), // circle_slice_7
    ('\u{f0aa5}', '●'), // circle_slice_8
    ('\u{f06b0}', '↑'), // update available
    ('\u{e0b0}', '▶'), // powerline separator
];

/// Used for PUA glyphs without a dedicated plain mapping.
const GENERIC_PLAIN_FALLBACK: char = '•';

fn is_supplementary_pua(c: char) -> bool {
    ('\u{f0000}'..='\u{10fffd}').contains(&c)
}

fn is_pua(c: char) -> bool {
    ('\u{e000}'..='\u{f8ff}').contains(&c) || is_supplementary_pua(c)
}

fn lookup(table: &[(char, char)], c: char, fallback: char) -> char {
    table
        .iter()
        .find(|(from, _)| *from == c)
        .map(|(_, to)| *to)
        .unwrap_or(fallback)
}

fn replace_chars<'a>(
    text: &'a str,
    needs_replacement: fn(char) -> bool,
    replacement: impl Fn(char) -> char,
) -> Cow<'a, str> {
    if !text.chars().any(needs_replacement) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(
        text.chars()
            .map(|c| {
                if needs_replacement(c) {
                    replacement(c)
                } else {
                    c
                }
            })
            .collect(),
    )
}

/// Replace supplementary-PUA icons with BMP Nerd Font glyphs.
/// Emoji, BMP icons and regular text are untouched.
pub fn to_bmp(text: &str) -> Cow<'_, str> {
    replace_chars(text, is_supplementary_pua, |c| lookup(BMP_FALLBACKS, c, GENERIC_BMP_FALLBACK))
}

/// Replace every Nerd Font (PUA) glyph with a plain Unicode symbol.
pub fn to_plain(text: &str) -> Cow<'_, str> {
    replace_chars(text, is_pua, |c| lookup(PLAIN_FALLBACKS, c, GENERIC_PLAIN_FALLBACK))
}

/// Whether Nerd Font glyphs can be rendered in this environment (cached per process).
pub fn nerd_font_usable() -> bool {
    static USABLE: OnceLock<bool> = OnceLock::new();
    *USABLE.get_or_init(detect_nerd_font)
}

fn detect_nerd_font() -> bool {
    let forced = std::env::var(ENV_OVERRIDE).ok();
    if let Some(usable) = env_override(forced.as_deref()) {
        return usable;
    }
    !cfg!(windows) || windows_font_dirs().iter().any(|dir| dir_has_nerd_font(dir))
}

fn env_override(value: Option<&str>) -> Option<bool> {
    let value = value?.trim().to_ascii_lowercase();
    match value.as_str() {
        "" => None,
        "0" | "false" | "no" | "off" => Some(false),
        _ => Some(true),
    }
}

/// System-wide and per-user font folders on Windows.
fn windows_font_dirs() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Ok(windir) = std::env::var("WINDIR") {
        folders.push(PathBuf::from(windir).join("Fonts"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        folders.push(
            PathBuf::from(local)
                .join("Microsoft")
                .join("Windows")
                .join("Fonts"),
        );
    }
    folders
}

fn dir_has_nerd_font(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| is_nerd_font_file(&entry.file_name().to_string_lossy()))
        })
        .unwrap_or(false)
}

/// Nerd Font files are named like `JetBrainsMonoNerdFont-Regular.ttf` (v3)
/// or `JetBrains Mono Regular Nerd Font Complete.ttf` (v2).
fn is_nerd_font_file(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.contains("nerdfont") || name.contains("nerd font")
}

/// Make rendered statusline text displayable on the current platform.
pub fn platform_safe(text: &str) -> Cow<'_, str> {
    if !nerd_font_usable() {
        to_plain(text)
    } else if cfg!(windows) {
        to_bmp(text)
    } else {
        Cow::Borrowed(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- to_bmp -------------------------------------------------------------

    #[test]
    fn maps_known_material_icons_to_bmp() {
        assert_eq!(to_bmp("\u{f024b}"), "\u{f07b}");
        assert_eq!(to_bmp("\u{f02a2}"), "\u{e725}");
        assert_eq!(to_bmp("\u{f0a9e}"), "\u{f10c}");
        assert_eq!(to_bmp("\u{f0aa1}"), "\u{f042}");
        assert_eq!(to_bmp("\u{f0aa5}"), "\u{f111}");
    }

    #[test]
    fn unknown_supplementary_pua_uses_generic_bmp_fallback() {
        assert_eq!(to_bmp("\u{f1234}"), GENERIC_BMP_FALLBACK.to_string());
        assert_eq!(to_bmp("\u{10fffd}"), GENERIC_BMP_FALLBACK.to_string());
    }

    #[test]
    fn to_bmp_leaves_bmp_icons_emoji_and_text_untouched() {
        for text in [
            "\u{e26d}",
            "\u{f49b}",
            "\u{e0b0}",
            "📁 🌿 ⚡️",
            "main ✓ 中文",
            "",
        ] {
            assert!(matches!(to_bmp(text), Cow::Borrowed(_)));
            assert_eq!(to_bmp(text), text);
        }
    }

    #[test]
    fn to_bmp_replaces_only_pua_chars_inside_mixed_text() {
        let input = "\u{f06b0} v1.2.0 \u{f02a2} main";
        assert_eq!(to_bmp(input), "\u{f021} v1.2.0 \u{e725} main");
    }

    #[test]
    fn every_bmp_mapping_targets_a_bmp_private_use_glyph() {
        for (from, to) in BMP_FALLBACKS {
            assert!(is_supplementary_pua(*from));
            assert!(('\u{e000}'..='\u{f8ff}').contains(to));
        }
        assert!(('\u{e000}'..='\u{f8ff}').contains(&GENERIC_BMP_FALLBACK));
    }

    // --- to_plain -----------------------------------------------------------

    #[test]
    fn maps_runtime_glyphs_to_plain_symbols() {
        assert_eq!(to_plain("\u{f0a9e}"), "○");
        assert_eq!(to_plain("\u{f0aa1}"), "◑");
        assert_eq!(to_plain("\u{f0aa5}"), "●");
        assert_eq!(to_plain("\u{f06b0} v1.2.0"), "↑ v1.2.0");
        assert_eq!(to_plain("\u{e0b0}"), "▶");
    }

    #[test]
    fn to_plain_replaces_bmp_and_supplementary_pua_with_generic_symbol() {
        assert_eq!(to_plain("\u{e26d}"), GENERIC_PLAIN_FALLBACK.to_string());
        assert_eq!(to_plain("\u{f49b}"), GENERIC_PLAIN_FALLBACK.to_string());
        assert_eq!(to_plain("\u{f1234}"), GENERIC_PLAIN_FALLBACK.to_string());
    }

    #[test]
    fn to_plain_keeps_ansi_codes_emoji_and_text() {
        assert_eq!(
            to_plain("\x1b[95m\u{f0aa0}\x1b[0m 26% 18%"),
            "\x1b[95m◔\x1b[0m 26% 18%"
        );
        for text in ["📁 🌿 ⚡️", "main ✓ 中文", "27% · 200k tokens", ""] {
            assert!(matches!(to_plain(text), Cow::Borrowed(_)));
        }
    }

    #[test]
    fn every_plain_mapping_targets_a_non_pua_symbol() {
        for (from, to) in PLAIN_FALLBACKS {
            assert!(is_pua(*from));
            assert!(!is_pua(*to));
        }
        assert!(!is_pua(GENERIC_PLAIN_FALLBACK));
    }

    // --- Nerd Font detection ------------------------------------------------

    #[test]
    fn env_override_parses_truthy_and_falsy_values() {
        assert_eq!(env_override(None), None);
        assert_eq!(env_override(Some("")), None);
        assert_eq!(env_override(Some("  ")), None);
        for falsy in ["0", "false", "FALSE", "no", " off "] {
            assert_eq!(env_override(Some(falsy)), Some(false), "{falsy:?}");
        }
        for truthy in ["1", "true", "yes", "on", "anything"] {
            assert_eq!(env_override(Some(truthy)), Some(true), "{truthy:?}");
        }
    }

    #[test]
    fn recognises_nerd_font_file_names_from_v2_and_v3() {
        for name in [
            "JetBrainsMonoNerdFont-Regular.ttf",
            "CaskaydiaCoveNerdFontMono-Bold.ttf",
            "SymbolsNerdFont-Regular.ttf",
            "Fira Code Regular Nerd Font Complete Windows Compatible.ttf",
            "HACK REGULAR NERD FONT COMPLETE.TTF",
        ] {
            assert!(is_nerd_font_file(name), "{name}");
        }
        for name in [
            "consola.ttf",
            "CascadiaMono.ttf",
            "segoeui.ttf",
            "Arial.ttf",
            "",
        ] {
            assert!(!is_nerd_font_file(name), "{name}");
        }
    }

    #[test]
    fn scans_font_dir_for_nerd_fonts() {
        let dir = std::env::temp_dir().join(format!("ccline-icons-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("consola.ttf"), b"").unwrap();
        assert!(!dir_has_nerd_font(&dir));

        std::fs::write(dir.join("JetBrainsMonoNerdFont-Regular.ttf"), b"").unwrap();
        assert!(dir_has_nerd_font(&dir));

        std::fs::remove_dir_all(&dir).unwrap();
        assert!(!dir_has_nerd_font(&dir));
    }

    #[test]
    fn platform_safe_matches_target_os_without_override() {
        if std::env::var(ENV_OVERRIDE).is_ok() {
            return;
        }
        let icon = "\u{f024b}";
        let safe = platform_safe(icon);
        if cfg!(windows) {
            assert!(safe == "\u{f07b}" || safe == "•");
        } else {
            assert_eq!(safe, icon);
        }
    }
}
