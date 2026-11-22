use super::{Segment, SegmentData};
use crate::config::{InputData, ModelConfig, SegmentId, TranscriptEntry};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

const TRANSCRIPT_TAIL_BYTES: u64 = 256 * 1024;

pub struct ContextWindowSegment {
    model_config: Option<ModelConfig>,
    dynamic_color: bool,
}

impl Default for ContextWindowSegment {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextWindowSegment {
    pub fn new() -> Self {
        Self {
            model_config: None,
            dynamic_color: true,
        }
    }

    pub fn with_model_config(mut self, model_config: ModelConfig) -> Self {
        self.model_config = Some(model_config);
        self
    }

    pub fn with_dynamic_color(mut self, dynamic_color: bool) -> Self {
        self.dynamic_color = dynamic_color;
        self
    }
}

impl Segment for ContextWindowSegment {
    fn collect(&self, input: &InputData) -> Option<SegmentData> {
        let loaded;
        let model_config = match &self.model_config {
            Some(cfg) => cfg,
            None => {
                loaded = ModelConfig::load();
                &loaded
            }
        };

        // Prefer the window Claude Code reports for this session; fall back to model ID rules
        let context_limit =
            model_config.get_context_limit(&input.model.id, input.native_context_limit());

        let usage_opt = resolve_usage(input, context_limit);

        let (percentage_display, tokens_display) = match usage_opt {
            Some((context_used_rate, context_used_token)) => {
                let percentage = if context_used_rate.fract() == 0.0 {
                    format!("{:.0}%", context_used_rate)
                } else {
                    format!("{:.1}%", context_used_rate)
                };

                let tokens = if context_used_token >= 1000 {
                    let k_value = context_used_token as f64 / 1000.0;
                    if k_value.fract() == 0.0 {
                        format!("{}k", k_value as u32)
                    } else {
                        format!("{:.1}k", k_value)
                    }
                } else {
                    context_used_token.to_string()
                };

                (percentage, tokens)
            }
            None => {
                // No usage data available
                ("-".to_string(), "-".to_string())
            }
        };

        let mut metadata = HashMap::new();
        match usage_opt {
            Some((context_used_rate, context_used_token)) => {
                metadata.insert("tokens".to_string(), context_used_token.to_string());
                metadata.insert("percentage".to_string(), context_used_rate.to_string());
                if self.dynamic_color {
                    if context_used_rate >= 80.0 {
                        metadata.insert("severity".to_string(), "crit".to_string());
                    } else if context_used_rate >= 60.0 {
                        metadata.insert("severity".to_string(), "warn".to_string());
                    }
                }
            }
            None => {
                metadata.insert("tokens".to_string(), "-".to_string());
                metadata.insert("percentage".to_string(), "-".to_string());
            }
        }
        metadata.insert("limit".to_string(), context_limit.to_string());
        metadata.insert("model".to_string(), input.model.id.clone());

        Some(SegmentData {
            primary: format!("{} · {} tokens", percentage_display, tokens_display),
            secondary: String::new(),
            metadata,
        })
    }

    fn id(&self) -> SegmentId {
        SegmentId::ContextWindow
    }
}

fn calc_percentage(tokens: u32, context_limit: u32) -> f64 {
    if context_limit == 0 {
        0.0
    } else {
        (tokens as f64 / context_limit as f64) * 100.0
    }
}

fn resolve_usage(input: &InputData, context_limit: u32) -> Option<(f64, u32)> {
    if let Some(cw) = &input.context_window {
        if let Some(usage) = &cw.current_usage {
            let tokens = usage.active_context_tokens();
            let native_size_matches = cw
                .context_window_size
                .is_none_or(|size| size == 0 || size == context_limit);
            let pct = match cw.used_percentage {
                Some(p) if native_size_matches => p,
                _ => calc_percentage(tokens, context_limit),
            };
            return Some((pct, tokens));
        }

        if let Some(pct) = cw.used_percentage {
            let tokens = ((pct / 100.0) * context_limit as f64).round().max(0.0) as u32;
            return Some((pct, tokens));
        }
    }

    if input.transcript_path.is_empty() {
        return None;
    }

    let tokens = try_parse_transcript_file(Path::new(&input.transcript_path))?;
    Some((calc_percentage(tokens, context_limit), tokens))
}

fn read_tail_lines(path: &Path, max_bytes: u64) -> Option<(Vec<String>, bool)> {
    let mut file = fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len == 0 {
        return None;
    }

    let truncated = len > max_bytes;
    if truncated {
        file.seek(SeekFrom::End(-(max_bytes as i64))).ok()?;
    }

    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;

    let slice = if truncated {
        // Drop the leading partial line after seeking into the middle of the file.
        match buf.iter().position(|&b| b == b'\n') {
            Some(pos) => &buf[pos + 1..],
            None => return None,
        }
    } else {
        &buf[..]
    };

    let text = String::from_utf8_lossy(slice);
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    if lines.is_empty() {
        None
    } else {
        Some((lines, truncated))
    }
}

fn try_parse_transcript_file(path: &Path) -> Option<u32> {
    let (tail_lines, truncated) = read_tail_lines(path, TRANSCRIPT_TAIL_BYTES)?;
    if let Some(usage) = find_usage_in_lines(&tail_lines, path) {
        return Some(usage);
    }

    if !truncated {
        return None;
    }

    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader
        .lines()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();
    find_usage_in_lines(&lines, path)
}

fn find_usage_in_lines(lines: &[String], path: &Path) -> Option<u32> {
    // Check if the last non-empty line is a summary
    let last_line = lines.iter().rev().find(|l| !l.trim().is_empty())?.trim();
    if let Ok(entry) = serde_json::from_str::<TranscriptEntry>(last_line) {
        if entry.r#type.as_deref() == Some("summary") {
            // Handle summary case: find usage by leafUuid
            if let Some(leaf_uuid) = &entry.leaf_uuid {
                let project_dir = path.parent()?;
                return find_usage_by_leaf_uuid(leaf_uuid, project_dir);
            }
        }
    }

    // Normal case: find the last assistant message in current file
    for line in lines.iter().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Ok(entry) = serde_json::from_str::<TranscriptEntry>(line) {
            if entry.r#type.as_deref() == Some("assistant") {
                if let Some(message) = &entry.message {
                    if let Some(raw_usage) = &message.usage {
                        let normalized = raw_usage.clone().normalize();
                        return Some(normalized.display_tokens());
                    }
                }
            }
        }
    }

    None
}

fn find_usage_by_leaf_uuid(leaf_uuid: &str, project_dir: &Path) -> Option<u32> {
    // Search for the leafUuid across all session files in the project directory
    let entries = fs::read_dir(project_dir).ok()?;

    for entry in entries {
        let entry = entry.ok()?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }

        if let Some(usage) = search_uuid_in_file(&path, leaf_uuid) {
            return Some(usage);
        }
    }

    None
}

fn search_uuid_in_file(path: &Path, target_uuid: &str) -> Option<u32> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader
        .lines()
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default();

    // Find the message with target_uuid
    for line in &lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Ok(entry) = serde_json::from_str::<TranscriptEntry>(line) {
            if let Some(uuid) = &entry.uuid {
                if uuid == target_uuid {
                    // Found the target message, check its type
                    if entry.r#type.as_deref() == Some("assistant") {
                        // Direct assistant message with usage
                        if let Some(message) = &entry.message {
                            if let Some(raw_usage) = &message.usage {
                                let normalized = raw_usage.clone().normalize();
                                return Some(normalized.display_tokens());
                            }
                        }
                    } else if entry.r#type.as_deref() == Some("user") {
                        // User message, need to find the parent assistant message
                        if let Some(parent_uuid) = &entry.parent_uuid {
                            return find_assistant_message_by_uuid(&lines, parent_uuid);
                        }
                    }
                    break;
                }
            }
        }
    }

    None
}

fn find_assistant_message_by_uuid(lines: &[String], target_uuid: &str) -> Option<u32> {
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Ok(entry) = serde_json::from_str::<TranscriptEntry>(line) {
            if let Some(uuid) = &entry.uuid {
                if uuid == target_uuid && entry.r#type.as_deref() == Some("assistant") {
                    if let Some(message) = &entry.message {
                        if let Some(raw_usage) = &message.usage {
                            let normalized = raw_usage.clone().normalize();
                            return Some(normalized.display_tokens());
                        }
                    }
                }
            }
        }
    }

    None
}
