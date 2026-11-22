use crate::config::{AnsiColor, Config, SegmentConfig, StyleMode};
use crate::core::icons;
use crate::core::segments::SegmentData;
use unicode_width::UnicodeWidthStr;

/// Columns Claude Code keeps clear on each side of the status line (`paddingX: 2`).
const CLAUDE_FOOTER_PADDING: usize = 2;

/// Strip ANSI CSI/OSC escape sequences and return terminal display width.
fn visible_width(text: &str) -> usize {
    let mut visible = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            match chars.peek() {
                Some('[') => {
                    // CSI sequence: ESC [ ... <0x40..=0x7E>
                    chars.next();
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    // OSC sequence: ESC ] ... (BEL or ST)
                    chars.next();
                    while let Some(c) = chars.next() {
                        if c == '\x07' {
                            break;
                        }
                        if c == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else {
            visible.push(ch);
        }
    }

    UnicodeWidthStr::width(visible.as_str())
}

pub struct StatusLineGenerator {
    config: Config,
}

impl StatusLineGenerator {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Usable status line width from `COLUMNS`, minus Claude Code's footer padding.
    /// Returns `None` when `COLUMNS` is unset (keeps single-line output).
    pub fn terminal_width_budget() -> Option<usize> {
        let cols: usize = std::env::var("COLUMNS").ok()?.trim().parse().ok()?;
        let budget = cols.saturating_sub(2 * CLAUDE_FOOTER_PADDING);
        (budget >= 20).then_some(budget)
    }

    pub fn generate(&self, segments: Vec<(SegmentConfig, SegmentData)>) -> String {
        self.generate_with_width(segments, None)
    }

    /// Render the status line, wrapping cleanly onto multiple lines between segments
    /// when `max_width` is given and the single-line output would exceed it.
    pub fn generate_with_width(
        &self,
        segments: Vec<(SegmentConfig, SegmentData)>,
        max_width: Option<usize>,
    ) -> String {
        let mut output = Vec::new();
        let mut rendered_configs = Vec::new();
        let enabled_segments: Vec<_> = segments
            .into_iter()
            .filter(|(config, _)| config.enabled)
            .collect();

        for (config, data) in &enabled_segments {
            let rendered = self.render_segment(config, data);
            if !rendered.is_empty() {
                output.push(rendered);
                rendered_configs.push(config);
            }
        }

        if output.is_empty() {
            return String::new();
        }

        let single_line = self.join_row(&output, &rendered_configs);
        let Some(budget) = max_width else {
            return single_line;
        };
        if visible_width(&single_line) <= budget {
            return single_line;
        }

        let sep_width = visible_width(&icons::platform_safe(&self.config.style.separator));

        let mut lines = Vec::new();
        let mut row_start = 0;
        let mut row_width = 0;

        for (i, seg) in output.iter().enumerate() {
            let seg_w = visible_width(seg);
            let added_w = if i == row_start {
                seg_w
            } else {
                sep_width + seg_w
            };

            if i > row_start && row_width + added_w > budget {
                let row = self.join_row(&output[row_start..i], &rendered_configs[row_start..i]);
                lines.push(row);
                row_start = i;
                row_width = seg_w;
            } else {
                row_width += added_w;
            }
        }

        let row = self.join_row(&output[row_start..], &rendered_configs[row_start..]);
        lines.push(row);
        lines.join("\n")
    }

    fn join_row(&self, row_segments: &[String], row_configs: &[&SegmentConfig]) -> String {
        // Handle Powerline arrow separators with color transition
        if self.config.style.separator == "\u{e0b0}" {
            self.join_with_powerline_arrows(row_segments, row_configs)
        } else {
            // For all other separators, use white color and simple join
            self.join_with_white_separators(row_segments)
        }
    }

    /// Generate statusline for TUI preview with proper width calculation
    /// This method handles ANSI escape sequences properly for ratatui rendering
    pub fn generate_for_tui(
        &self,
        segments: Vec<(SegmentConfig, SegmentData)>,
    ) -> ratatui::text::Line<'static> {
        use ansi_to_tui::IntoText;
        use ratatui::text::{Line, Span};

        // Use the same generate method and convert to TUI
        let full_output = self.generate(segments);

        if let Ok(text) = full_output.into_text() {
            if let Some(line) = text.lines.into_iter().next() {
                return line;
            }
        }

        // Fallback to raw text
        Line::from(vec![Span::raw(full_output)])
    }

    /// Generate TUI-optimized text with intelligent wrapping by segment for preview
    pub fn generate_for_tui_preview(
        &self,
        segments: Vec<(SegmentConfig, SegmentData)>,
        max_width: u16,
    ) -> ratatui::text::Text<'_> {
        use ansi_to_tui::IntoText;
        use ratatui::text::{Line, Span, Text};

        let enabled_segments: Vec<_> = segments
            .into_iter()
            .filter(|(config, _)| config.enabled)
            .collect();

        if enabled_segments.is_empty() {
            return Text::from(vec![Line::default()]);
        }

        // Render each segment individually
        let mut rendered_segments = Vec::new();
        let mut segment_configs = Vec::new();

        for (config, data) in &enabled_segments {
            let rendered = self.render_segment(config, data);
            if !rendered.is_empty() {
                rendered_segments.push(rendered);
                segment_configs.push(config.clone());
            }
        }

        if rendered_segments.is_empty() {
            return Text::from(vec![Line::default()]);
        }

        // Pre-calculate separators between segments
        let mut separators = Vec::new();
        for i in 0..rendered_segments.len().saturating_sub(1) {
            let separator = if self.config.style.separator == "\u{e0b0}" {
                // Powerline arrows with color transition
                let prev_bg = segment_configs
                    .get(i)
                    .and_then(|config| config.colors.background.as_ref());
                let curr_bg = segment_configs
                    .get(i + 1)
                    .and_then(|config| config.colors.background.as_ref());
                self.create_powerline_arrow(prev_bg, curr_bg)
            } else {
                // Regular separators with white color
                let safe_separator = icons::platform_safe(&self.config.style.separator);
                format!("\x1b[37m{}\x1b[0m", safe_separator)
            };
            separators.push(separator);
        }

        // Intelligent line wrapping by segment
        let mut lines: Vec<String> = Vec::new();
        let mut current_line = String::new();
        let mut current_width = 0usize;
        let max_w = max_width as usize;

        for i in 0..rendered_segments.len() {
            let segment = &rendered_segments[i];
            let segment_width = visible_width(segment);

            // Check if adding this segment would exceed max_width
            if current_width > 0 && current_width + segment_width > max_w {
                // Current line would overflow, start a new line
                lines.push(current_line.clone());
                current_line.clear();
                current_width = 0;
            }

            // Add the segment to current line
            current_line.push_str(segment);
            current_width += segment_width;

            // Handle separator if not the last segment
            if i < separators.len() {
                let separator = &separators[i];
                let separator_width = visible_width(separator);

                // Check if next segment exists
                if i + 1 < rendered_segments.len() {
                    let next_segment = &rendered_segments[i + 1];
                    let next_width = visible_width(next_segment);

                    // Check if separator AND next segment both fit
                    if current_width + separator_width + next_width <= max_w {
                        // Both fit, add separator and continue on same line
                        current_line.push_str(separator);
                        current_width += separator_width;
                    } else {
                        // Separator and/or next segment don't fit
                        // Don't add separator, just break line
                        lines.push(current_line.clone());
                        current_line.clear();
                        current_width = 0;
                    }
                }
            }
        }

        // Add the last line if it's not empty
        if !current_line.is_empty() {
            lines.push(current_line);
        }

        // Convert string lines to ratatui Text
        let mut tui_lines = Vec::new();
        for line in lines {
            if let Ok(text) = line.into_text() {
                for tui_line in text.lines {
                    tui_lines.push(tui_line);
                }
            } else {
                tui_lines.push(Line::from(vec![Span::raw(line)]));
            }
        }

        // Ensure we have at least one line
        if tui_lines.is_empty() {
            tui_lines.push(Line::default());
        }

        Text::from(tui_lines)
    }

    fn render_segment(&self, config: &SegmentConfig, data: &SegmentData) -> String {
        let rendered = self.render_segment_raw(config, data);
        icons::platform_safe(&rendered).into_owned()
    }

    fn render_segment_raw(&self, config: &SegmentConfig, data: &SegmentData) -> String {
        let icon = if let Some(dynamic_icon) = data.metadata.get("dynamic_icon") {
            dynamic_icon.clone()
        } else {
            self.get_icon(config)
        };

        // Apply background color to the entire segment if set
        if let Some(bg_color) = &config.colors.background {
            let bg_code = self.apply_background_color(bg_color);
            let reset_to_bg = format!("\x1b[0m{}", bg_code);

            // Build the entire segment content first
            let icon_colored = if let Some(icon_color) = &config.colors.icon {
                self.apply_color(&icon, Some(icon_color))
                    .replace("\x1b[0m", &reset_to_bg)
            } else {
                icon.clone()
            };

            let text_styled = self
                .apply_style(
                    &data.primary,
                    config.colors.text.as_ref(),
                    config.styles.text_bold,
                )
                .replace("\x1b[0m", &reset_to_bg);

            let mut segment_content = format!(" {} {} ", icon_colored, text_styled);

            if !data.secondary.is_empty() {
                let secondary_styled = self
                    .apply_style(
                        &data.secondary,
                        config.colors.text.as_ref(),
                        config.styles.text_bold,
                    )
                    .replace("\x1b[0m", &reset_to_bg);
                segment_content.push_str(&format!("{} ", secondary_styled));
            }

            // Apply background to the entire content and reset all attributes at the end
            format!("{}{}\x1b[0m", bg_code, segment_content)
        } else {
            // No background color: allow severity-driven warning/critical color overrides
            let severity_color = match data.metadata.get("severity").map(String::as_str) {
                Some("warn") => Some(AnsiColor::Color16 { c16: 11 }),
                Some("crit") => Some(AnsiColor::Color16 { c16: 9 }),
                _ => None,
            };
            let icon_color = severity_color.as_ref().or(config.colors.icon.as_ref());
            let text_color = severity_color.as_ref().or(config.colors.text.as_ref());

            let icon_colored = self.apply_color(&icon, icon_color);
            let text_styled = self.apply_style(&data.primary, text_color, config.styles.text_bold);

            let mut segment = format!("{} {}", icon_colored, text_styled);

            if !data.secondary.is_empty() {
                segment.push_str(&format!(
                    " {}",
                    self.apply_style(&data.secondary, text_color, config.styles.text_bold)
                ));
            }

            segment
        }
    }

    fn get_icon(&self, config: &SegmentConfig) -> String {
        let wants_nerd_font = matches!(
            self.config.style.mode,
            StyleMode::NerdFont | StyleMode::Powerline
        );
        // Without a Nerd Font the PUA glyphs cannot render, so use the emoji variant
        if wants_nerd_font && icons::nerd_font_usable() {
            config.icon.nerd_font.clone()
        } else {
            config.icon.plain.clone()
        }
    }

    fn apply_color(&self, text: &str, color: Option<&AnsiColor>) -> String {
        match color {
            Some(AnsiColor::Color16 { c16 }) => {
                let code = if *c16 < 8 { 30 + c16 } else { 90 + (c16 - 8) };
                format!("\x1b[{}m{}\x1b[0m", code, text)
            }
            Some(AnsiColor::Color256 { c256 }) => {
                format!("\x1b[38;5;{}m{}\x1b[0m", c256, text)
            }
            Some(AnsiColor::Rgb { r, g, b }) => {
                format!("\x1b[38;2;{};{};{}m{}\x1b[0m", r, g, b, text)
            }
            None => text.to_string(),
        }
    }

    fn apply_style(&self, text: &str, color: Option<&AnsiColor>, bold: bool) -> String {
        let mut codes = Vec::new();

        // Add style codes
        if bold {
            codes.push("1".to_string()); // Bold: \x1b[1m
        }

        // Add color codes
        match color {
            Some(AnsiColor::Color16 { c16 }) => {
                let color_code = if *c16 < 8 { 30 + c16 } else { 90 + (c16 - 8) };
                codes.push(color_code.to_string());
            }
            Some(AnsiColor::Color256 { c256 }) => {
                codes.push("38".to_string());
                codes.push("5".to_string());
                codes.push(c256.to_string());
            }
            Some(AnsiColor::Rgb { r, g, b }) => {
                codes.push("38".to_string());
                codes.push("2".to_string());
                codes.push(r.to_string());
                codes.push(g.to_string());
                codes.push(b.to_string());
            }
            None => {}
        }

        if codes.is_empty() {
            text.to_string()
        } else {
            format!("\x1b[{}m{}\x1b[0m", codes.join(";"), text)
        }
    }

    fn apply_background_color(&self, color: &AnsiColor) -> String {
        match color {
            AnsiColor::Color16 { c16 } => {
                let code = if *c16 < 8 { 40 + c16 } else { 100 + (c16 - 8) };
                format!("\x1b[{}m", code)
            }
            AnsiColor::Color256 { c256 } => {
                format!("\x1b[48;5;{}m", c256)
            }
            AnsiColor::Rgb { r, g, b } => {
                format!("\x1b[48;2;{};{};{}m", r, g, b)
            }
        }
    }

    /// Join segments with white separators (non-Powerline)
    fn join_with_white_separators(&self, rendered_segments: &[String]) -> String {
        if rendered_segments.is_empty() {
            return String::new();
        }

        // Use white color for separator
        let safe_separator = icons::platform_safe(&self.config.style.separator);
        let white_separator = format!("\x1b[37m{}\x1b[0m", safe_separator);
        rendered_segments.join(&white_separator)
    }

    /// Join segments with Powerline arrow separators with proper color transitions
    fn join_with_powerline_arrows(
        &self,
        rendered_segments: &[String],
        segment_configs: &[&SegmentConfig],
    ) -> String {
        if rendered_segments.is_empty() {
            return String::new();
        }

        if rendered_segments.len() == 1 {
            return rendered_segments[0].clone();
        }

        let mut result = rendered_segments[0].clone();

        for (i, segment) in rendered_segments.iter().enumerate().skip(1) {
            let prev_bg = segment_configs
                .get(i - 1)
                .and_then(|config| config.colors.background.as_ref());
            let curr_bg = segment_configs
                .get(i)
                .and_then(|config| config.colors.background.as_ref());

            // Create Powerline arrow with color transition
            let arrow = self.create_powerline_arrow(prev_bg, curr_bg);

            result.push_str(&arrow);
            result.push_str(segment);
        }

        // Reset colors at the end
        result.push_str("\x1b[0m");
        result
    }

    /// Create a Powerline arrow with proper color transition
    fn create_powerline_arrow(
        &self,
        prev_bg: Option<&AnsiColor>,
        curr_bg: Option<&AnsiColor>,
    ) -> String {
        let arrow_char = icons::platform_safe("\u{e0b0}");

        match (prev_bg, curr_bg) {
            (Some(prev), Some(curr)) => {
                // Arrow foreground = previous segment's background
                // Arrow background = current segment's background
                let fg_code = self.color_to_foreground_code(prev);
                let bg_code = self.apply_background_color(curr);
                format!("{}{}{}\x1b[0m", bg_code, fg_code, arrow_char)
            }
            (Some(prev), None) => {
                // Previous segment has background, current doesn't
                let fg_code = self.color_to_foreground_code(prev);
                format!("{}{}\x1b[0m", fg_code, arrow_char)
            }
            (None, Some(curr)) => {
                // Current segment has background, previous doesn't
                let bg_code = self.apply_background_color(curr);
                format!("{}{}\x1b[0m", bg_code, arrow_char)
            }
            (None, None) => {
                // Neither segment has background color
                arrow_char.into_owned()
            }
        }
    }

    /// Convert AnsiColor to foreground color code
    fn color_to_foreground_code(&self, color: &AnsiColor) -> String {
        match color {
            AnsiColor::Color16 { c16 } => {
                let code = if *c16 < 8 { 30 + c16 } else { 90 + (c16 - 8) };
                format!("\x1b[{}m", code)
            }
            AnsiColor::Color256 { c256 } => {
                format!("\x1b[38;5;{}m", c256)
            }
            AnsiColor::Rgb { r, g, b } => {
                format!("\x1b[38;2;{};{};{}m", r, g, b)
            }
        }
    }
}

pub fn collect_all_segments(
    config: &Config,
    input: &crate::config::InputData,
) -> Vec<(SegmentConfig, SegmentData)> {
    use crate::config::{ModelConfig, SegmentId};
    use crate::core::segments::*;

    let needs_model_config = config
        .segments
        .iter()
        .any(|s| s.enabled && matches!(s.id, SegmentId::Model | SegmentId::ContextWindow));
    let model_config = needs_model_config.then(ModelConfig::load);

    let mut results = Vec::new();

    for segment_config in &config.segments {
        // Skip disabled segments to avoid unnecessary API requests
        if !segment_config.enabled {
            continue;
        }

        let segment_data = match segment_config.id {
            SegmentId::Model => {
                let show_effort = segment_config
                    .options
                    .get("show_effort")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let segment = match &model_config {
                    Some(cfg) => ModelSegment::new().with_model_config(cfg.clone()),
                    None => ModelSegment::new(),
                };
                segment.with_effort(show_effort).collect(input)
            }
            SegmentId::Directory => {
                let segment = DirectorySegment::new();
                segment.collect(input)
            }
            SegmentId::Git => {
                let show_sha = segment_config
                    .options
                    .get("show_sha")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let segment = GitSegment::new().with_sha(show_sha);
                segment.collect(input)
            }
            SegmentId::ContextWindow => {
                let dynamic_color = segment_config
                    .options
                    .get("dynamic_color")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let segment = match &model_config {
                    Some(cfg) => ContextWindowSegment::new().with_model_config(cfg.clone()),
                    None => ContextWindowSegment::new(),
                };
                segment.with_dynamic_color(dynamic_color).collect(input)
            }
            SegmentId::Cache => {
                let segment = CacheSegment::new();
                segment.collect(input)
            }
            SegmentId::Usage => {
                let segment = UsageSegment::new().with_options(&segment_config.options);
                segment.collect(input)
            }
            SegmentId::Cost => {
                let segment = CostSegment::new();
                segment.collect(input)
            }
            SegmentId::Session => {
                let segment = SessionSegment::new();
                segment.collect(input)
            }
            SegmentId::OutputStyle => {
                let segment = OutputStyleSegment::new();
                segment.collect(input)
            }
            SegmentId::Update => {
                let segment = UpdateSegment::new();
                segment.collect(input)
            }
        };

        if let Some(data) = segment_data {
            results.push((segment_config.clone(), data));
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::themes::ThemePresets;
    use std::collections::HashMap;

    #[test]
    fn background_segment_restores_bg_around_inner_resets_and_resets_all_at_end() {
        let config = ThemePresets::get_powerline_dark();
        let session_cfg = config
            .segments
            .iter()
            .find(|s| s.id == crate::config::SegmentId::Session)
            .unwrap()
            .clone();
        let generator = StatusLineGenerator::new(config);
        let data = SegmentData {
            primary: "3m".to_string(),
            secondary: "\x1b[32m+10\x1b[0m \x1b[31m-2\x1b[0m".to_string(),
            metadata: HashMap::new(),
        };

        let rendered = generator.render_segment_raw(&session_cfg, &data);
        assert!(rendered.ends_with("\x1b[0m"));
        assert!(!rendered.ends_with("\x1b[49m"));
        assert!(rendered.contains("\x1b[32m+10\x1b[0m\x1b[48;2;45;50;59m"));
        assert!(rendered.contains("\x1b[31m-2\x1b[0m\x1b[48;2;45;50;59m"));
    }

    #[test]
    fn auto_wraps_onto_two_lines_only_when_narrower_than_content() {
        let config = ThemePresets::get_cometix();
        let generator = StatusLineGenerator::new(config.clone());
        let segments: Vec<(SegmentConfig, SegmentData)> = config
            .segments
            .iter()
            .take(4)
            .cloned()
            .zip(["Opus 4.6 1M", "my-project", "main ✓", "42% · 84k tokens"])
            .map(|(cfg, text)| {
                (
                    cfg,
                    SegmentData {
                        primary: text.to_string(),
                        secondary: String::new(),
                        metadata: HashMap::new(),
                    },
                )
            })
            .collect();

        // Wide terminal stays on one line
        let wide = generator.generate_with_width(segments.clone(), Some(120));
        assert!(!wide.contains('\n'));

        // Narrow terminal wraps cleanly between segments without trailing separators
        let narrow = generator.generate_with_width(segments, Some(40));
        let rows: Vec<&str> = narrow.lines().collect();
        assert!(rows.len() >= 2);
        for row in rows {
            assert!(visible_width(row) <= 40);
            assert!(!row.trim_end().ends_with('|'));
        }
    }

    #[test]
    fn severity_metadata_overrides_text_color_when_no_background() {
        let config = ThemePresets::get_cometix();
        let cw_cfg = config
            .segments
            .iter()
            .find(|s| s.id == crate::config::SegmentId::ContextWindow)
            .unwrap()
            .clone();
        let generator = StatusLineGenerator::new(config);
        let mut metadata = HashMap::new();
        metadata.insert("severity".to_string(), "crit".to_string());
        let data = SegmentData {
            primary: "85% · 170k tokens".to_string(),
            secondary: String::new(),
            metadata,
        };

        let rendered = generator.render_segment_raw(&cw_cfg, &data);
        // ANSI bright red (c16: 9 -> code 91)
        assert!(rendered.contains("91m"));
    }
}
