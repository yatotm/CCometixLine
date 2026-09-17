//! Platform icon compatibility.
//!
//! Nerd Font's Material Design set lives in Supplementary Private Use Area-A
//! (U+F0000+). Those code points need UTF-16 surrogate pairs, which Windows
//! consoles and older Nerd Font builds frequently render as `?`. On Windows we
//! swap them for equivalent BMP glyphs (Font Awesome / Devicons, present in
//! every Nerd Font version); other platforms are left untouched.

use std::borrow::Cow;

/// Material Design icon → BMP Nerd Font equivalent.
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

/// Used for supplementary-PUA icons without a dedicated mapping.
const GENERIC_FALLBACK: char = '\u{f111}'; // fa-circle

fn is_supplementary_pua(c: char) -> bool {
    ('\u{f0000}'..='\u{10fffd}').contains(&c)
}

fn bmp_equivalent(c: char) -> char {
    BMP_FALLBACKS
        .iter()
        .find(|(from, _)| *from == c)
        .map(|(_, to)| *to)
        .unwrap_or(GENERIC_FALLBACK)
}

/// Replace every supplementary-PUA icon in `text` with a BMP equivalent.
/// Emoji and regular text are untouched.
pub fn to_bmp(text: &str) -> Cow<'_, str> {
    if !text.chars().any(is_supplementary_pua) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(
        text.chars()
            .map(|c| {
                if is_supplementary_pua(c) {
                    bmp_equivalent(c)
                } else {
                    c
                }
            })
            .collect(),
    )
}

/// Apply the BMP fallback only where it is needed (Windows).
pub fn platform_safe(text: &str) -> Cow<'_, str> {
    if cfg!(windows) {
        to_bmp(text)
    } else {
        Cow::Borrowed(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_material_icons_to_bmp() {
        assert_eq!(to_bmp("\u{f024b}"), "\u{f07b}");
        assert_eq!(to_bmp("\u{f02a2}"), "\u{e725}");
        assert_eq!(to_bmp("\u{f0a9e}"), "\u{f10c}");
        assert_eq!(to_bmp("\u{f0aa1}"), "\u{f042}");
        assert_eq!(to_bmp("\u{f0aa5}"), "\u{f111}");
    }

    #[test]
    fn unknown_supplementary_pua_uses_generic_fallback() {
        assert_eq!(to_bmp("\u{f1234}"), GENERIC_FALLBACK.to_string());
        assert_eq!(to_bmp("\u{10fffd}"), GENERIC_FALLBACK.to_string());
    }

    #[test]
    fn leaves_bmp_icons_emoji_and_text_untouched() {
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
    fn replaces_only_pua_chars_inside_mixed_text() {
        let input = "\u{f06b0} v1.2.0 \u{f02a2} main";
        assert_eq!(to_bmp(input), "\u{f021} v1.2.0 \u{e725} main");
    }

    #[test]
    fn every_mapping_targets_a_bmp_private_use_glyph() {
        for (from, to) in BMP_FALLBACKS {
            assert!(is_supplementary_pua(*from));
            assert!(('\u{e000}'..='\u{f8ff}').contains(to));
        }
        assert!(('\u{e000}'..='\u{f8ff}').contains(&GENERIC_FALLBACK));
    }

    #[test]
    fn platform_safe_matches_target_os() {
        let icon = "\u{f024b}";
        if cfg!(windows) {
            assert_eq!(platform_safe(icon), "\u{f07b}");
        } else {
            assert_eq!(platform_safe(icon), icon);
        }
    }
}
