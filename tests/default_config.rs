use ccometixline::config::{Config, SegmentId, StyleMode};
use ccometixline::ui::themes::ThemePresets;

#[test]
fn default_config_is_the_cometix_theme() {
    let config = Config::default();
    assert_eq!(config.theme, "cometix");
    assert_eq!(config.style.mode, StyleMode::NerdFont);
    assert_eq!(config.style.separator, " | ");
}

#[test]
fn default_config_enables_every_segment_in_order() {
    let config = Config::default();
    let ids: Vec<SegmentId> = config.segments.iter().map(|s| s.id).collect();
    assert_eq!(
        ids,
        vec![
            SegmentId::Model,
            SegmentId::Directory,
            SegmentId::Git,
            SegmentId::ContextWindow,
            SegmentId::Usage,
            SegmentId::Cost,
            SegmentId::Session,
            SegmentId::OutputStyle,
        ]
    );
    assert!(config.segments.iter().all(|s| s.enabled));
}

#[test]
fn default_config_keeps_segment_options() {
    let config = Config::default();
    let usage = config
        .segments
        .iter()
        .find(|s| s.id == SegmentId::Usage)
        .expect("usage segment");
    assert_eq!(
        usage.options.get("api_base_url").and_then(|v| v.as_str()),
        Some("https://api.anthropic.com")
    );
    assert_eq!(
        usage.options.get("cache_duration").and_then(|v| v.as_u64()),
        Some(180)
    );
    assert_eq!(
        usage.options.get("timeout").and_then(|v| v.as_u64()),
        Some(2)
    );

    let git = config
        .segments
        .iter()
        .find(|s| s.id == SegmentId::Git)
        .expect("git segment");
    assert_eq!(
        git.options.get("show_sha").and_then(|v| v.as_bool()),
        Some(false)
    );
}

#[test]
fn builtin_cometix_preset_matches_default_config() {
    // The TUI "modified from theme" check compares against the preset, so both must agree.
    // Compare against the built-in preset directly: get_theme() would read ~/.claude first.
    let config = Config::default();
    let preset = ThemePresets::get_cometix();
    assert_eq!(config.theme, preset.theme);
    assert_eq!(config.style.mode, preset.style.mode);
    assert_eq!(config.style.separator, preset.style.separator);
    assert_eq!(config.segments.len(), preset.segments.len());
    for (actual, expected) in config.segments.iter().zip(preset.segments.iter()) {
        assert_eq!(actual.id, expected.id);
        assert_eq!(actual.enabled, expected.enabled);
        assert_eq!(actual.icon.nerd_font, expected.icon.nerd_font);
        assert_eq!(actual.options, expected.options);
    }
}
