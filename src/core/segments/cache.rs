use super::{Segment, SegmentData};
use crate::config::{InputData, SegmentId};
use chrono::Utc;
use std::collections::HashMap;

#[derive(Default)]
pub struct CacheSegment;

impl CacheSegment {
    pub fn new() -> Self {
        Self
    }

    fn format_countdown(secs: i64) -> String {
        if secs >= 3_600 {
            let hours = secs / 3_600;
            let mins = (secs % 3_600) / 60;
            if mins == 0 {
                format!("{}h", hours)
            } else {
                format!("{}h{}m", hours, mins)
            }
        } else if secs >= 60 {
            let mins = secs / 60;
            let rem = secs % 60;
            if mins >= 10 || rem == 0 {
                format!("{}m", mins)
            } else {
                format!("{}m{}s", mins, rem)
            }
        } else {
            format!("{}s", secs.max(0))
        }
    }

    pub fn collect_at(&self, input: &InputData, now_secs: i64) -> Option<SegmentData> {
        let pc = input.prompt_cache.as_ref()?;
        if pc.caching_observed == Some(false) {
            return None;
        }

        let ttl_secs: i64 = match pc.ttl.as_deref() {
            Some("1h") => 3_600,
            _ => 300,
        };

        let left = pc
            .expires_at
            .map(|t| t as i64 - now_secs)
            .filter(|&l| l > 0 && pc.warm == Some(true));

        let mut metadata = HashMap::new();
        let primary = match left {
            Some(secs) => {
                metadata.insert("state".to_string(), "warm".to_string());
                metadata.insert("remaining_secs".to_string(), secs.to_string());
                let warn_threshold = (ttl_secs / 5).clamp(60, 300);
                if secs < warn_threshold {
                    metadata.insert("severity".to_string(), "warn".to_string());
                }
                Self::format_countdown(secs)
            }
            None => {
                metadata.insert("state".to_string(), "cold".to_string());
                metadata.insert("severity".to_string(), "warn".to_string());
                "cold".to_string()
            }
        };

        // The first request only writes the cache, so its 0% hit ratio is noise.
        let secondary = pc
            .hit_ratio
            .filter(|_| pc.requests.unwrap_or(0) > 1)
            .map(|ratio| {
                let pct = if ratio <= 1.0 { ratio * 100.0 } else { ratio };
                let clamped = pct.clamp(0.0, 100.0).round() as u32;
                metadata.insert("hit_ratio".to_string(), clamped.to_string());
                format!("· {}%", clamped)
            })
            .unwrap_or_default();

        Some(SegmentData {
            primary,
            secondary,
            metadata,
        })
    }
}

impl Segment for CacheSegment {
    fn collect(&self, input: &InputData) -> Option<SegmentData> {
        self.collect_at(input, Utc::now().timestamp())
    }

    fn id(&self) -> SegmentId {
        SegmentId::Cache
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_warm_cache_with_hit_ratio() {
        let input: InputData = serde_json::from_str(
            r#"{
                "prompt_cache": {
                    "warm": true,
                    "caching_observed": true,
                    "ttl": "1h",
                    "expires_at": 1700003000,
                    "requests": 12,
                    "hit_ratio": 0.97
                }
            }"#,
        )
        .unwrap();

        let seg = CacheSegment::new();
        let data = seg.collect_at(&input, 1_700_000_480).expect("cache data");
        assert_eq!(data.primary, "42m");
        assert_eq!(data.secondary, "· 97%");
        assert!(data.metadata.get("severity").is_none());
    }

    #[test]
    fn warns_when_expiring_soon_or_cold_and_hides_first_turn_hit_ratio() {
        let seg = CacheSegment::new();
        let expiring: InputData = serde_json::from_str(
            r#"{
                "prompt_cache": {
                    "warm": true,
                    "ttl": "5m",
                    "expires_at": 1700000045,
                    "requests": 1,
                    "hit_ratio": 0.0
                }
            }"#,
        )
        .unwrap();

        let data = seg.collect_at(&expiring, 1_700_000_000).expect("expiring cache");
        assert_eq!(data.primary, "45s");
        assert_eq!(data.secondary, "");
        assert_eq!(
            data.metadata.get("severity").map(String::as_str),
            Some("warn")
        );

        let cold: InputData = serde_json::from_str(
            r#"{
                "prompt_cache": {
                    "warm": false,
                    "expires_at": 1699999000,
                    "requests": 5,
                    "hit_ratio": 0.8
                }
            }"#,
        )
        .unwrap();
        let data = seg.collect_at(&cold, 1_700_000_000).expect("cold cache");
        assert_eq!(data.primary, "cold");
        assert_eq!(data.secondary, "· 80%");
        assert_eq!(
            data.metadata.get("severity").map(String::as_str),
            Some("warn")
        );
    }

    #[test]
    fn hides_when_caching_not_observed() {
        let input: InputData = serde_json::from_str(
            r#"{"prompt_cache": {"caching_observed": false, "warm": false}}"#,
        )
        .unwrap();
        let res = CacheSegment::new().collect_at(&input, 1_700_000_000);
        assert!(res.is_none());
    }
}
