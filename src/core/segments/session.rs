use super::{Segment, SegmentData};
use crate::config::{InputData, SegmentId};
use std::collections::HashMap;

const MAX_SESSION_NAME_CHARS: usize = 20;

pub struct SessionSegment {
    show_name: bool,
}

impl Default for SessionSegment {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionSegment {
    pub fn new() -> Self {
        Self { show_name: true }
    }

    pub fn with_name(mut self, show_name: bool) -> Self {
        self.show_name = show_name;
        self
    }

    fn format_duration(ms: u64) -> String {
        if ms < 1000 {
            format!("{}ms", ms)
        } else if ms < 60_000 {
            let seconds = ms / 1000;
            format!("{}s", seconds)
        } else if ms < 3_600_000 {
            let minutes = ms / 60_000;
            let seconds = (ms % 60_000) / 1000;
            if seconds == 0 {
                format!("{}m", minutes)
            } else {
                format!("{}m{}s", minutes, seconds)
            }
        } else {
            let hours = ms / 3_600_000;
            let minutes = (ms % 3_600_000) / 60_000;
            if minutes == 0 {
                format!("{}h", hours)
            } else {
                format!("{}h{}m", hours, minutes)
            }
        }
    }

    fn truncate_name(name: &str) -> String {
        let count = name.chars().count();
        if count <= MAX_SESSION_NAME_CHARS {
            name.to_string()
        } else {
            let prefix: String = name.chars().take(MAX_SESSION_NAME_CHARS - 1).collect();
            format!("{}…", prefix)
        }
    }
}

impl Segment for SessionSegment {
    fn collect(&self, input: &InputData) -> Option<SegmentData> {
        let cost_data = input.cost.as_ref();
        let duration_ms = cost_data
            .and_then(|c| c.total_duration_ms)
            .map(|ms| ms.max(0.0).round() as u64);
        let session_name = if self.show_name {
            input
                .session_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(Self::truncate_name)
        } else {
            None
        };

        let primary = match (session_name.as_deref(), duration_ms) {
            (Some(name), Some(ms)) => format!("{} · {}", name, Self::format_duration(ms)),
            (None, Some(ms)) => Self::format_duration(ms),
            (Some(name), None) => name.to_string(),
            (None, None) => return None,
        };

        // Secondary display: line changes if available (green for +, red for -)
        let secondary = match cost_data.map(|c| (c.total_lines_added, c.total_lines_removed)) {
            Some((Some(added), Some(removed))) if added > 0 || removed > 0 => {
                format!("\x1b[32m+{}\x1b[0m \x1b[31m-{}\x1b[0m", added, removed)
            }
            Some((Some(added), None)) if added > 0 => {
                format!("\x1b[32m+{}\x1b[0m", added)
            }
            Some((None, Some(removed))) if removed > 0 => {
                format!("\x1b[31m-{}\x1b[0m", removed)
            }
            _ => String::new(),
        };

        let mut metadata = HashMap::new();
        if let Some(name) = session_name {
            metadata.insert("session_name".to_string(), name);
        }
        if let Some(duration) = duration_ms {
            metadata.insert("duration_ms".to_string(), duration.to_string());
        }
        if let Some(api_duration) = cost_data
            .and_then(|c| c.total_api_duration_ms)
            .map(|ms| ms.max(0.0).round() as u64)
        {
            metadata.insert("api_duration_ms".to_string(), api_duration.to_string());
        }
        if let Some(added) = cost_data.and_then(|c| c.total_lines_added) {
            metadata.insert("lines_added".to_string(), added.to_string());
        }
        if let Some(removed) = cost_data.and_then(|c| c.total_lines_removed) {
            metadata.insert("lines_removed".to_string(), removed.to_string());
        }

        Some(SegmentData {
            primary,
            secondary,
            metadata,
        })
    }

    fn id(&self) -> SegmentId {
        SegmentId::Session
    }
}
