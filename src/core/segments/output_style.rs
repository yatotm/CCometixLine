use super::{Segment, SegmentData};
use crate::config::{InputData, SegmentId};
use std::collections::HashMap;

#[derive(Default)]
pub struct OutputStyleSegment;

impl OutputStyleSegment {
    pub fn new() -> Self {
        Self
    }
}

impl Segment for OutputStyleSegment {
    fn collect(&self, input: &InputData) -> Option<SegmentData> {
        let style_name = input
            .output_style
            .as_ref()
            .map(|s| s.name.trim())
            .filter(|s| !s.is_empty());
        let vim_mode = input
            .vim
            .as_ref()
            .and_then(|v| v.mode.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty());

        let mut metadata = HashMap::new();
        if let Some(style) = style_name {
            metadata.insert("style_name".to_string(), style.to_string());
        }
        if let Some(vim) = vim_mode {
            metadata.insert("vim_mode".to_string(), vim.to_string());
        }

        let (primary, secondary) = match (style_name, vim_mode) {
            (Some(style), Some(vim)) => (style.to_string(), format!("[{}]", vim)),
            (Some(style), None) => (style.to_string(), String::new()),
            (None, Some(vim)) => (format!("[{}]", vim), String::new()),
            (None, None) => return None,
        };

        Some(SegmentData {
            primary,
            secondary,
            metadata,
        })
    }

    fn id(&self) -> SegmentId {
        SegmentId::OutputStyle
    }
}
