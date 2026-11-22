use super::{Segment, SegmentData};
use crate::config::{InputData, ModelConfig, SegmentId};
use std::collections::HashMap;

pub struct ModelSegment {
    model_config: Option<ModelConfig>,
    show_effort: bool,
}

impl Default for ModelSegment {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelSegment {
    pub fn new() -> Self {
        Self {
            model_config: None,
            show_effort: true,
        }
    }

    pub fn with_model_config(mut self, model_config: ModelConfig) -> Self {
        self.model_config = Some(model_config);
        self
    }

    pub fn with_effort(mut self, show_effort: bool) -> Self {
        self.show_effort = show_effort;
        self
    }
}

impl Segment for ModelSegment {
    fn collect(&self, input: &InputData) -> Option<SegmentData> {
        if input.model.id.is_empty() && input.model.display_name.is_empty() {
            return None;
        }

        let mut metadata = HashMap::new();
        metadata.insert("model_id".to_string(), input.model.id.clone());
        metadata.insert("display_name".to_string(), input.model.display_name.clone());

        let mut secondary_parts = Vec::new();
        if self.show_effort {
            if let Some(effort) = Self::format_effort(input) {
                metadata.insert("effort".to_string(), effort.clone());
                secondary_parts.push(format!("· {}", effort));
            }
        }
        if input.fast_mode == Some(true) {
            metadata.insert("fast_mode".to_string(), "true".to_string());
            secondary_parts.push("· Fast".to_string());
        }
        if let Some(agent) = input
            .agent
            .as_ref()
            .and_then(|a| a.name.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            metadata.insert("agent".to_string(), agent.to_string());
            secondary_parts.push(format!("@{}", agent));
        }

        Some(SegmentData {
            primary: self.format_model_name(input),
            secondary: secondary_parts.join(" "),
            metadata,
        })
    }

    fn id(&self) -> SegmentId {
        SegmentId::Model
    }
}

impl ModelSegment {
    fn format_effort(input: &InputData) -> Option<String> {
        if input.thinking.as_ref().and_then(|t| t.enabled) == Some(false) {
            return None;
        }
        let raw = input.effort.as_ref()?.level.as_deref()?.trim();
        if raw.is_empty() {
            return None;
        }
        let lower = raw.to_ascii_lowercase();
        let formatted = match lower.as_str() {
            "low" => "Low".to_string(),
            "medium" => "Med".to_string(),
            "high" => "High".to_string(),
            "xhigh" => "xHigh".to_string(),
            "max" => "Max".to_string(),
            _ => {
                let mut chars = lower.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                    None => return None,
                }
            }
        };
        Some(formatted)
    }

    fn format_model_name(&self, input: &InputData) -> String {
        let id = &input.model.id;
        let display_name = &input.model.display_name;
        let loaded;
        let model_config = match &self.model_config {
            Some(cfg) => cfg,
            None => {
                loaded = ModelConfig::load();
                &loaded
            }
        };

        let (config_name, limit, modifier_suffix) =
            model_config.resolve(id, input.native_context_limit());

        let name = if let Some(config_name) = config_name {
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
            match &modifier_suffix {
                Some(suffix) => format!("{}{}", base, suffix),
                None => base,
            }
        };

        // 1M active without a [1m] suffix (new Claude Code / Fable): mark it the same way
        if modifier_suffix.is_none()
            && limit >= 1_000_000
            && !name.to_ascii_lowercase().contains("1m")
        {
            format!("{} 1M", name)
        } else {
            name
        }
    }
}
