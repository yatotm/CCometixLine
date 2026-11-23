use super::segment_list::{FieldSelection, Panel};
use crate::config::{Config, SegmentConfig, SegmentId, StyleMode};
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionKind {
    Bool,
    Uint,
    String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OptionItem {
    pub key: String,
    pub value: serde_json::Value,
    pub kind: OptionKind,
}

pub struct SettingsViewState<'a> {
    pub selected_segment: usize,
    pub selected_panel: &'a Panel,
    pub selected_field: &'a FieldSelection,
    pub options_expanded: bool,
    pub editing_option: Option<&'a str>,
}

impl OptionItem {
    fn bool(key: &str, value: bool) -> Self {
        Self {
            key: key.to_string(),
            value: serde_json::Value::Bool(value),
            kind: OptionKind::Bool,
        }
    }

    fn uint(key: &str, value: u64) -> Self {
        Self {
            key: key.to_string(),
            value: serde_json::Value::from(value),
            kind: OptionKind::Uint,
        }
    }

    fn string(key: &str, value: &str) -> Self {
        Self {
            key: key.to_string(),
            value: serde_json::Value::String(value.to_string()),
            kind: OptionKind::String,
        }
    }
}

fn default_options(id: SegmentId) -> Vec<OptionItem> {
    let mut items = Vec::new();
    match id {
        SegmentId::Model => items.push(OptionItem::bool("show_effort", true)),
        SegmentId::Git => items.push(OptionItem::bool("show_sha", false)),
        SegmentId::ContextWindow => items.push(OptionItem::bool("dynamic_color", true)),
        SegmentId::Session => items.push(OptionItem::bool("show_name", true)),
        SegmentId::Usage => {
            let url = "https://api.anthropic.com";
            items.push(OptionItem::string("api_base_url", url));
            items.push(OptionItem::uint("cache_duration", 300));
            items.push(OptionItem::uint("timeout", 2));
        }
        _ => {}
    }
    items
}

/// Ordered configurable options for a segment: known defaults merged with any custom keys.
pub fn effective_options(segment: &SegmentConfig) -> Vec<OptionItem> {
    let mut items = default_options(segment.id);
    for item in &mut items {
        if let Some(val) = segment.options.get(&item.key) {
            item.value = val.clone();
        }
    }

    let mut extra_keys: Vec<&String> = segment
        .options
        .keys()
        .filter(|k| !items.iter().any(|item| &item.key == *k))
        .collect();
    extra_keys.sort();

    for key in extra_keys {
        if let Some(value) = segment.options.get(key) {
            let kind = if value.is_boolean() {
                OptionKind::Bool
            } else if value.is_u64() {
                OptionKind::Uint
            } else {
                OptionKind::String
            };
            items.push(OptionItem {
                key: key.clone(),
                value: value.clone(),
                kind,
            });
        }
    }

    items
}

pub fn format_option_raw(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

#[derive(Default)]
pub struct SettingsComponent;

impl SettingsComponent {
    pub fn new() -> Self {
        Self
    }

    pub fn render(&self, f: &mut Frame, area: Rect, config: &Config, state: &SettingsViewState) {
        let selected_segment = state.selected_segment;
        let selected_panel = state.selected_panel;
        let selected_field = state.selected_field;
        if let Some(segment) = config.segments.get(selected_segment) {
            let segment_name = match segment.id {
                SegmentId::Model => "Model",
                SegmentId::Directory => "Directory",
                SegmentId::Git => "Git",
                SegmentId::ContextWindow => "Context Window",
                SegmentId::Cache => "Cache",
                SegmentId::Usage => "Usage",
                SegmentId::Cost => "Cost",
                SegmentId::Session => "Session",
                SegmentId::OutputStyle => "Output Style",
                SegmentId::Update => "Update",
            };
            let current_icon = match config.style.mode {
                StyleMode::Plain => &segment.icon.plain,
                StyleMode::NerdFont | StyleMode::Powerline => &segment.icon.nerd_font,
            };
            // Convert AnsiColor to ratatui Color
            let icon_ratatui_color = match &segment.colors.icon {
                Some(crate::config::AnsiColor::Color16 { c16 }) => match c16 {
                    0 => Color::Black,
                    1 => Color::Red,
                    2 => Color::Green,
                    3 => Color::Yellow,
                    4 => Color::Blue,
                    5 => Color::Magenta,
                    6 => Color::Cyan,
                    7 => Color::White,
                    8 => Color::DarkGray,
                    9 => Color::LightRed,
                    10 => Color::LightGreen,
                    11 => Color::LightYellow,
                    12 => Color::LightBlue,
                    13 => Color::LightMagenta,
                    14 => Color::LightCyan,
                    15 => Color::Gray,
                    _ => Color::White,
                },
                Some(crate::config::AnsiColor::Color256 { c256 }) => Color::Indexed(*c256),
                Some(crate::config::AnsiColor::Rgb { r, g, b }) => Color::Rgb(*r, *g, *b),
                None => Color::White,
            };
            let text_ratatui_color = match &segment.colors.text {
                Some(crate::config::AnsiColor::Color16 { c16 }) => match c16 {
                    0 => Color::Black,
                    1 => Color::Red,
                    2 => Color::Green,
                    3 => Color::Yellow,
                    4 => Color::Blue,
                    5 => Color::Magenta,
                    6 => Color::Cyan,
                    7 => Color::White,
                    8 => Color::DarkGray,
                    9 => Color::LightRed,
                    10 => Color::LightGreen,
                    11 => Color::LightYellow,
                    12 => Color::LightBlue,
                    13 => Color::LightMagenta,
                    14 => Color::LightCyan,
                    15 => Color::Gray,
                    _ => Color::White,
                },
                Some(crate::config::AnsiColor::Color256 { c256 }) => Color::Indexed(*c256),
                Some(crate::config::AnsiColor::Rgb { r, g, b }) => Color::Rgb(*r, *g, *b),
                None => Color::White,
            };
            let icon_color_desc = match &segment.colors.icon {
                Some(crate::config::AnsiColor::Color16 { c16 }) => match c16 {
                    0 => "Black".to_string(),
                    1 => "Red".to_string(),
                    2 => "Green".to_string(),
                    3 => "Yellow".to_string(),
                    4 => "Blue".to_string(),
                    5 => "Magenta".to_string(),
                    6 => "Cyan".to_string(),
                    7 => "White".to_string(),
                    8 => "Dark Gray".to_string(),
                    9 => "Light Red".to_string(),
                    10 => "Light Green".to_string(),
                    11 => "Light Yellow".to_string(),
                    12 => "Light Blue".to_string(),
                    13 => "Light Magenta".to_string(),
                    14 => "Light Cyan".to_string(),
                    15 => "Gray".to_string(),
                    _ => format!("ANSI {}", c16),
                },
                Some(crate::config::AnsiColor::Color256 { c256 }) => format!("256:{}", c256),
                Some(crate::config::AnsiColor::Rgb { r, g, b }) => {
                    format!("RGB({},{},{})", r, g, b)
                }
                None => "Default".to_string(),
            };
            let text_color_desc = match &segment.colors.text {
                Some(crate::config::AnsiColor::Color16 { c16 }) => match c16 {
                    0 => "Black".to_string(),
                    1 => "Red".to_string(),
                    2 => "Green".to_string(),
                    3 => "Yellow".to_string(),
                    4 => "Blue".to_string(),
                    5 => "Magenta".to_string(),
                    6 => "Cyan".to_string(),
                    7 => "White".to_string(),
                    8 => "Dark Gray".to_string(),
                    9 => "Light Red".to_string(),
                    10 => "Light Green".to_string(),
                    11 => "Light Yellow".to_string(),
                    12 => "Light Blue".to_string(),
                    13 => "Light Magenta".to_string(),
                    14 => "Light Cyan".to_string(),
                    15 => "Gray".to_string(),
                    _ => format!("ANSI {}", c16),
                },
                Some(crate::config::AnsiColor::Color256 { c256 }) => format!("256:{}", c256),
                Some(crate::config::AnsiColor::Rgb { r, g, b }) => {
                    format!("RGB({},{},{})", r, g, b)
                }
                None => "Default".to_string(),
            };
            let background_ratatui_color = match &segment.colors.background {
                Some(crate::config::AnsiColor::Color16 { c16 }) => match c16 {
                    0 => Color::Black,
                    1 => Color::Red,
                    2 => Color::Green,
                    3 => Color::Yellow,
                    4 => Color::Blue,
                    5 => Color::Magenta,
                    6 => Color::Cyan,
                    7 => Color::White,
                    8 => Color::DarkGray,
                    9 => Color::LightRed,
                    10 => Color::LightGreen,
                    11 => Color::LightYellow,
                    12 => Color::LightBlue,
                    13 => Color::LightMagenta,
                    14 => Color::LightCyan,
                    15 => Color::Gray,
                    _ => Color::White,
                },
                Some(crate::config::AnsiColor::Color256 { c256 }) => Color::Indexed(*c256),
                Some(crate::config::AnsiColor::Rgb { r, g, b }) => Color::Rgb(*r, *g, *b),
                None => Color::White,
            };
            let background_color_desc = match &segment.colors.background {
                Some(crate::config::AnsiColor::Color16 { c16 }) => match c16 {
                    0 => "Black".to_string(),
                    1 => "Red".to_string(),
                    2 => "Green".to_string(),
                    3 => "Yellow".to_string(),
                    4 => "Blue".to_string(),
                    5 => "Magenta".to_string(),
                    6 => "Cyan".to_string(),
                    7 => "White".to_string(),
                    8 => "Dark Gray".to_string(),
                    9 => "Light Red".to_string(),
                    10 => "Light Green".to_string(),
                    11 => "Light Yellow".to_string(),
                    12 => "Light Blue".to_string(),
                    13 => "Light Magenta".to_string(),
                    14 => "Light Cyan".to_string(),
                    15 => "Gray".to_string(),
                    _ => format!("ANSI {}", c16),
                },
                Some(crate::config::AnsiColor::Color256 { c256 }) => format!("256:{}", c256),
                Some(crate::config::AnsiColor::Rgb { r, g, b }) => {
                    format!("RGB({},{},{})", r, g, b)
                }
                None => "None".to_string(),
            };
            let create_field_line = |field: FieldSelection, content: Vec<Span<'static>>| {
                let is_selected = *selected_panel == Panel::Settings && *selected_field == field;
                let mut spans = vec![];

                if is_selected {
                    spans.push(Span::styled(
                        "▶ ".to_string(),
                        Style::default().fg(Color::Cyan),
                    ));
                } else {
                    spans.push(Span::raw("  ".to_string()));
                }

                spans.extend(content);
                Line::from(spans)
            };
            let options = effective_options(segment);
            let options_label = if options.is_empty() {
                "└─ Options: None".to_string()
            } else {
                let arrow = if state.options_expanded { "▾" } else { "▸" };
                format!("└─ Options ({}) [{}]", options.len(), arrow)
            };

            let mut lines = vec![
                Line::from(format!("{} Segment", segment_name)),
                create_field_line(
                    FieldSelection::Enabled,
                    vec![Span::raw(format!(
                        "├─ Enabled: {}",
                        if segment.enabled { "✓" } else { "✗" }
                    ))],
                ),
                create_field_line(
                    FieldSelection::Icon,
                    vec![
                        Span::raw("├─ Icon: ".to_string()),
                        Span::styled(
                            current_icon.to_string(),
                            Style::default().fg(icon_ratatui_color),
                        ),
                    ],
                ),
                create_field_line(
                    FieldSelection::IconColor,
                    vec![
                        Span::raw(format!("├─ Icon Color: {} ", icon_color_desc)),
                        Span::styled("██".to_string(), Style::default().fg(icon_ratatui_color)),
                    ],
                ),
                create_field_line(
                    FieldSelection::TextColor,
                    vec![
                        Span::raw(format!("├─ Text Color: {} ", text_color_desc)),
                        Span::styled("██".to_string(), Style::default().fg(text_ratatui_color)),
                    ],
                ),
                create_field_line(
                    FieldSelection::BackgroundColor,
                    vec![
                        Span::raw(format!("├─ Background Color: {} ", background_color_desc)),
                        if segment.colors.background.is_some() {
                            Span::styled(
                                "██".to_string(),
                                Style::default().fg(background_ratatui_color),
                            )
                        } else {
                            Span::styled("--".to_string(), Style::default().fg(Color::DarkGray))
                        },
                    ],
                ),
                create_field_line(
                    FieldSelection::TextStyle,
                    vec![Span::raw(format!(
                        "├─ Text Style: Bold {}",
                        if segment.styles.text_bold {
                            "[✓]"
                        } else {
                            "[ ]"
                        }
                    ))],
                ),
                create_field_line(FieldSelection::Options, vec![Span::raw(options_label)]),
            ];

            if state.options_expanded && !options.is_empty() {
                let last_idx = options.len() - 1;
                for (idx, item) in options.iter().enumerate() {
                    let branch = if idx == last_idx {
                        "    └─ "
                    } else {
                        "    ├─ "
                    };
                    let field = FieldSelection::OptionItem(idx);
                    let is_editing = *selected_panel == Panel::Settings
                        && *selected_field == field
                        && state.editing_option.is_some();

                    let value_span = if let (true, Some(buf)) = (is_editing, state.editing_option) {
                        Span::styled(format!("{}█", buf), Style::default().fg(Color::Yellow))
                    } else if item.kind == OptionKind::Bool {
                        let enabled = item.value.as_bool().unwrap_or(false);
                        Span::raw(if enabled { "[✓]" } else { "[ ]" })
                    } else {
                        Span::raw(format_option_raw(&item.value))
                    };

                    let key_span = Span::raw(format!("{}{}: ", branch, item.key));
                    lines.push(create_field_line(field, vec![key_span, value_span]));
                }
            }
            let text = Text::from(lines);
            let settings_block = Block::default()
                .borders(Borders::ALL)
                .title("Settings")
                .border_style(if *selected_panel == Panel::Settings {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default()
                });
            let settings_panel = Paragraph::new(text).block(settings_block);
            f.render_widget(settings_panel, area);
        } else {
            let settings_block = Block::default()
                .borders(Borders::ALL)
                .title("Settings")
                .border_style(if *selected_panel == Panel::Settings {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default()
                });
            let settings_panel = Paragraph::new("No segment selected").block(settings_block);
            f.render_widget(settings_panel, area);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::themes::ThemePresets;

    #[test]
    fn effective_options_supplies_defaults_for_legacy_empty_options() {
        let config = ThemePresets::get_cometix();
        let mut session = config
            .segments
            .iter()
            .find(|s| s.id == SegmentId::Session)
            .unwrap()
            .clone();
        session.options.clear();

        let opts = effective_options(&session);
        assert_eq!(opts.len(), 1);
        assert_eq!(opts[0].key, "show_name");
        assert_eq!(opts[0].kind, OptionKind::Bool);
        assert_eq!(opts[0].value, serde_json::Value::Bool(true));

        let dir = config
            .segments
            .iter()
            .find(|s| s.id == SegmentId::Directory)
            .unwrap();
        assert!(effective_options(dir).is_empty());
    }

    #[test]
    fn effective_options_preserves_usage_order_and_custom_keys() {
        let config = ThemePresets::get_cometix();
        let mut usage = config
            .segments
            .iter()
            .find(|s| s.id == SegmentId::Usage)
            .unwrap()
            .clone();
        let opts = &mut usage.options;
        opts.insert("z_custom".to_string(), serde_json::Value::Bool(true));

        let opts = effective_options(&usage);
        let keys: Vec<&str> = opts.iter().map(|o| o.key.as_str()).collect();
        assert_eq!(
            keys,
            vec!["api_base_url", "cache_duration", "timeout", "z_custom"]
        );
        assert_eq!(opts[1].value, serde_json::Value::from(180u64));
    }
}
