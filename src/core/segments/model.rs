use super::{Segment, SegmentData};
use crate::config::{InputData, ModelConfig, SegmentId};
use std::collections::HashMap;

#[derive(Default)]
pub struct ModelSegment;

impl ModelSegment {
    pub fn new() -> Self {
        Self
    }
}

impl Segment for ModelSegment {
    fn collect(&self, input: &InputData) -> Option<SegmentData> {
        let mut metadata = HashMap::new();
        metadata.insert("model_id".to_string(), input.model.id.clone());
        metadata.insert("display_name".to_string(), input.model.display_name.clone());

        Some(SegmentData {
            primary: self.format_model_name(input),
            secondary: String::new(),
            metadata,
        })
    }

    fn id(&self) -> SegmentId {
        SegmentId::Model
    }
}

impl ModelSegment {
    fn format_model_name(&self, input: &InputData) -> String {
        let id = &input.model.id;
        let display_name = &input.model.display_name;
        let model_config = ModelConfig::load();

        let name = if let Some(config_name) = model_config.get_display_name(id) {
            // Model recognized by config, display_name already includes modifier suffix
            config_name
        } else {
            // Fallback: prefer upstream display_name, fall back to model_id if empty
            let base = if display_name.is_empty() {
                id.to_string()
            } else {
                display_name.to_string()
            };
            // Still apply context modifier suffix (e.g., " 1M") if present
            match model_config.get_display_suffix(id) {
                Some(suffix) => format!("{}{}", base, suffix),
                None => base,
            }
        };

        // 1M active without a [1m] suffix (new Claude Code / Fable): mark it the same way
        let has_modifier = model_config.get_display_suffix(id).is_some();
        let limit = model_config.get_context_limit(id, input.native_context_limit());
        if !has_modifier && limit >= 1_000_000 {
            format!("{} 1M", name)
        } else {
            name
        }
    }
}
