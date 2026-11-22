use super::{Segment, SegmentData};
use crate::config::{InputData, SegmentId};
use crate::utils::credentials;
use chrono::{DateTime, Datelike, Duration, Local, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const DEFAULT_API_BASE_URL: &str = "https://api.anthropic.com";
const DEFAULT_CACHE_DURATION_SECS: u64 = 300;
const DEFAULT_TIMEOUT_SECS: u64 = 2;
const RATE_LIMIT_BACKOFF_BASE_SECS: i64 = 60;
const RATE_LIMIT_BACKOFF_MAX_SECS: i64 = 900;
const ERROR_BACKOFF_BASE_SECS: i64 = 30;
const ERROR_BACKOFF_MAX_SECS: i64 = 300;

#[derive(Debug, Deserialize)]
struct ApiUsageResponse {
    five_hour: UsagePeriod,
    seven_day: UsagePeriod,
}

#[derive(Debug, Deserialize)]
struct UsagePeriod {
    utilization: f64,
    resets_at: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ApiUsageCache {
    #[serde(default)]
    five_hour_utilization: Option<f64>,
    #[serde(default)]
    seven_day_utilization: Option<f64>,
    #[serde(default)]
    resets_at: Option<String>,
    #[serde(default)]
    cached_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    retry_after: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    consecutive_failures: u32,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

#[derive(Debug, PartialEq, Eq)]
enum ProxySetting {
    Configured(String),
    None,
}

enum FetchOutcome {
    Ok(ApiUsageResponse),
    RateLimited,
    Failed,
    ProxyError,
}

pub struct UsageSegment {
    api_base_url: String,
    cache_duration: u64,
    timeout: u64,
}

impl Default for UsageSegment {
    fn default() -> Self {
        Self::new()
    }
}

impl UsageSegment {
    pub fn new() -> Self {
        Self {
            api_base_url: DEFAULT_API_BASE_URL.to_string(),
            cache_duration: DEFAULT_CACHE_DURATION_SECS,
            timeout: DEFAULT_TIMEOUT_SECS,
        }
    }

    pub fn with_options(mut self, options: &HashMap<String, serde_json::Value>) -> Self {
        if let Some(url) = options
            .get("api_base_url")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            self.api_base_url = url.to_string();
        }
        if let Some(duration) = options.get("cache_duration").and_then(|v| v.as_u64()) {
            self.cache_duration = duration;
        }
        if let Some(timeout) = options.get("timeout").and_then(|v| v.as_u64()) {
            self.timeout = timeout;
        }
        self
    }

    fn get_circle_icon(utilization: f64) -> String {
        let percent = (utilization.clamp(0.0, 1.0) * 100.0) as u8;
        match percent {
            0..=12 => "\u{f0a9e}".to_string(),  // circle_slice_1
            13..=25 => "\u{f0a9f}".to_string(), // circle_slice_2
            26..=37 => "\u{f0aa0}".to_string(), // circle_slice_3
            38..=50 => "\u{f0aa1}".to_string(), // circle_slice_4
            51..=62 => "\u{f0aa2}".to_string(), // circle_slice_5
            63..=75 => "\u{f0aa3}".to_string(), // circle_slice_6
            76..=87 => "\u{f0aa4}".to_string(), // circle_slice_7
            _ => "\u{f0aa5}".to_string(),       // circle_slice_8
        }
    }

    fn format_reset_time(reset_time_str: Option<&str>) -> String {
        if let Some(time_str) = reset_time_str {
            if let Ok(dt) = DateTime::parse_from_rfc3339(time_str) {
                let mut local_dt = dt.with_timezone(&Local);
                if local_dt.minute() > 45 {
                    local_dt += Duration::hours(1);
                }
                return format!(
                    "{}-{}-{}",
                    local_dt.month(),
                    local_dt.day(),
                    local_dt.hour()
                );
            }
        }
        "?".to_string()
    }

    fn claude_config_dir() -> Option<PathBuf> {
        if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
            let trimmed = dir.trim();
            if !trimmed.is_empty() {
                return Some(PathBuf::from(trimmed));
            }
        }
        dirs::home_dir().map(|home| home.join(".claude"))
    }

    fn get_cache_path() -> Option<PathBuf> {
        let config_dir = Self::claude_config_dir()?;
        Some(config_dir.join("ccline").join(".api_usage_cache.json"))
    }

    fn load_cache(&self) -> Option<ApiUsageCache> {
        let cache_path = Self::get_cache_path()?;
        if !cache_path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(&cache_path).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn save_cache(&self, cache: &ApiUsageCache) {
        if let Some(cache_path) = Self::get_cache_path() {
            if let Some(parent) = cache_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(cache) {
                let tmp_ext = format!("json.{}.tmp", std::process::id());
                let tmp_path = cache_path.with_extension(tmp_ext);
                if std::fs::write(&tmp_path, json).is_ok()
                    && std::fs::rename(&tmp_path, &cache_path).is_err()
                {
                    let _ = std::fs::remove_file(&tmp_path);
                }
            }
        }
    }

    fn is_cache_valid(&self, cache: &ApiUsageCache, now: DateTime<Utc>) -> bool {
        if cache.five_hour_utilization.is_none() || cache.seven_day_utilization.is_none() {
            return false;
        }
        cache
            .cached_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .is_some_and(|cached_at| {
                let elapsed = now.signed_duration_since(cached_at.with_timezone(&Utc));
                elapsed.num_seconds() < self.cache_duration as i64
            })
    }

    fn is_in_backoff(cache: &ApiUsageCache, now: DateTime<Utc>) -> bool {
        cache
            .retry_after
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .is_some_and(|until| now < until.with_timezone(&Utc))
    }

    fn should_skip_fetch(&self, cache: &ApiUsageCache, now: DateTime<Utc>) -> bool {
        self.is_cache_valid(cache, now) || Self::is_in_backoff(cache, now)
    }

    fn backoff_seconds(consecutive_failures: u32, rate_limited: bool) -> i64 {
        let shift = consecutive_failures.saturating_sub(1).min(6);
        let (base, max) = if rate_limited {
            (RATE_LIMIT_BACKOFF_BASE_SECS, RATE_LIMIT_BACKOFF_MAX_SECS)
        } else {
            (ERROR_BACKOFF_BASE_SECS, ERROR_BACKOFF_MAX_SECS)
        };
        (base << shift).min(max)
    }

    fn user_agent(input: &InputData) -> String {
        match input.version.as_deref().map(str::trim) {
            Some(v) if !v.is_empty() => format!("claude-code/{}", v),
            _ => "claude-code".to_string(),
        }
    }

    fn extract_proxy_from_json(content: &str) -> Option<String> {
        const PROXY_KEYS: &[&str] = &[
            "HTTPS_PROXY",
            "https_proxy",
            "HTTP_PROXY",
            "http_proxy",
            "ALL_PROXY",
            "all_proxy",
        ];

        let settings: serde_json::Value = serde_json::from_str(content).ok()?;
        let env = settings.get("env")?.as_object()?;
        for key in PROXY_KEYS {
            if let Some(val) = env.get(*key).and_then(|v| v.as_str()) {
                let trimmed = val.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
        None
    }

    fn get_proxy_from_settings_dir(config_dir: &Path) -> ProxySetting {
        for filename in ["settings.local.json", "settings.json"] {
            let path = config_dir.join(filename);
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Some(proxy) = Self::extract_proxy_from_json(&content) {
                    return ProxySetting::Configured(proxy);
                }
            }
        }
        ProxySetting::None
    }

    fn build_agent(proxy_setting: ProxySetting) -> Option<ureq::Agent> {
        match proxy_setting {
            ProxySetting::Configured(proxy_url) => {
                // Fail closed: if Claude Code settings configure a proxy that cannot be
                // parsed or initialized, abort instead of silently falling back to direct IP.
                let proxy = ureq::Proxy::new(&proxy_url).ok()?;
                let agent = ureq::Agent::config_builder()
                    .proxy(Some(proxy))
                    .build()
                    .new_agent();
                Some(agent)
            }
            ProxySetting::None => Some(ureq::Agent::new_with_defaults()),
        }
    }

    fn fetch_api_usage(&self, token: &str, user_agent: &str) -> FetchOutcome {
        let proxy_setting = Self::claude_config_dir()
            .map(|dir| Self::get_proxy_from_settings_dir(&dir))
            .unwrap_or(ProxySetting::None);
        let agent = match Self::build_agent(proxy_setting) {
            Some(agent) => agent,
            None => return FetchOutcome::ProxyError,
        };

        let url = format!("{}/api/oauth/usage", self.api_base_url);
        let response = agent
            .get(&url)
            .header("Authorization", &format!("Bearer {}", token))
            .header("anthropic-beta", "oauth-2025-04-20")
            .header("User-Agent", user_agent)
            .config()
            .timeout_global(Some(std::time::Duration::from_secs(self.timeout)))
            .build()
            .call();

        match response {
            Ok(resp) => match resp.into_body().read_json() {
                Ok(data) => FetchOutcome::Ok(data),
                Err(_) => FetchOutcome::Failed,
            },
            Err(ureq::Error::StatusCode(429)) => FetchOutcome::RateLimited,
            Err(_) => FetchOutcome::Failed,
        }
    }
}

impl Segment for UsageSegment {
    fn collect(&self, input: &InputData) -> Option<SegmentData> {
        let token = credentials::get_oauth_token()?;
        let now = Utc::now();
        let cached_data = self.load_cache();

        let skip_fetch = cached_data
            .as_ref()
            .is_some_and(|cache| self.should_skip_fetch(cache, now));

        let (five_hour_util, seven_day_util, resets_at) = if skip_fetch {
            let cache = cached_data?;
            (
                cache.five_hour_utilization?,
                cache.seven_day_utilization?,
                cache.resets_at,
            )
        } else {
            let ua = Self::user_agent(input);
            match self.fetch_api_usage(&token, &ua) {
                FetchOutcome::Ok(response) => {
                    let cache = ApiUsageCache {
                        five_hour_utilization: Some(response.five_hour.utilization),
                        seven_day_utilization: Some(response.seven_day.utilization),
                        resets_at: response.seven_day.resets_at.clone(),
                        cached_at: Some(now.to_rfc3339()),
                        retry_after: None,
                        consecutive_failures: 0,
                    };
                    self.save_cache(&cache);
                    (
                        response.five_hour.utilization,
                        response.seven_day.utilization,
                        response.seven_day.resets_at,
                    )
                }
                outcome @ (FetchOutcome::RateLimited | FetchOutcome::Failed) => {
                    let mut cache = cached_data.unwrap_or_default();
                    cache.consecutive_failures = cache.consecutive_failures.saturating_add(1);
                    let wait_secs = Self::backoff_seconds(
                        cache.consecutive_failures,
                        matches!(outcome, FetchOutcome::RateLimited),
                    );
                    cache.retry_after = Some((now + Duration::seconds(wait_secs)).to_rfc3339());
                    self.save_cache(&cache);
                    (
                        cache.five_hour_utilization?,
                        cache.seven_day_utilization?,
                        cache.resets_at,
                    )
                }
                FetchOutcome::ProxyError => {
                    let cache = cached_data?;
                    (
                        cache.five_hour_utilization?,
                        cache.seven_day_utilization?,
                        cache.resets_at,
                    )
                }
            }
        };

        let dynamic_icon = Self::get_circle_icon(seven_day_util / 100.0);
        let seven_day_percent = seven_day_util.round() as u8;
        let five_hour_percent = five_hour_util.round() as u8;
        let primary = format!("{}% {}%", seven_day_percent, five_hour_percent);
        let secondary = format!("· {}", Self::format_reset_time(resets_at.as_deref()));

        let mut metadata = HashMap::new();
        metadata.insert("dynamic_icon".to_string(), dynamic_icon);
        metadata.insert(
            "five_hour_utilization".to_string(),
            five_hour_util.to_string(),
        );
        metadata.insert(
            "seven_day_utilization".to_string(),
            seven_day_util.to_string(),
        );

        Some(SegmentData {
            primary,
            secondary,
            metadata,
        })
    }

    fn id(&self) -> SegmentId {
        SegmentId::Usage
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_uses_input_version() {
        let with_ver: InputData = serde_json::from_str(r#"{"version":"2.1.280"}"#).unwrap();
        assert_eq!(UsageSegment::user_agent(&with_ver), "claude-code/2.1.280");

        let without_ver: InputData = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(UsageSegment::user_agent(&without_ver), "claude-code");
    }

    #[test]
    fn extracts_proxy_case_insensitively_with_priority() {
        let both =
            r#"{"env":{"HTTPS_PROXY":"http://127.0.0.1:7890","HTTP_PROXY":"http://127.0.0.1:8080"}}"#;
        assert_eq!(
            UsageSegment::extract_proxy_from_json(both).as_deref(),
            Some("http://127.0.0.1:7890")
        );

        let lower = r#"{"env":{"https_proxy":"socks5://127.0.0.1:1080"}}"#;
        assert_eq!(
            UsageSegment::extract_proxy_from_json(lower).as_deref(),
            Some("socks5://127.0.0.1:1080")
        );

        let fallback = r#"{"env":{"HTTPS_PROXY":"  ","all_proxy":"socks5h://127.0.0.1:1080"}}"#;
        assert_eq!(
            UsageSegment::extract_proxy_from_json(fallback).as_deref(),
            Some("socks5h://127.0.0.1:1080")
        );

        assert_eq!(UsageSegment::extract_proxy_from_json(r#"{"env":{}}"#), None);
    }

    #[test]
    fn settings_local_overrides_settings_json_proxy() {
        let dir = std::env::temp_dir().join(format!("ccline-proxy-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(
            dir.join("settings.json"),
            r#"{"env":{"HTTPS_PROXY":"http://127.0.0.1:7890"}}"#,
        )
        .unwrap();
        assert_eq!(
            UsageSegment::get_proxy_from_settings_dir(&dir),
            ProxySetting::Configured("http://127.0.0.1:7890".to_string())
        );

        std::fs::write(
            dir.join("settings.local.json"),
            r#"{"env":{"https_proxy":"socks5://127.0.0.1:1080"}}"#,
        )
        .unwrap();
        assert_eq!(
            UsageSegment::get_proxy_from_settings_dir(&dir),
            ProxySetting::Configured("socks5://127.0.0.1:1080".to_string())
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn build_agent_fails_closed_on_invalid_proxy_and_supports_socks5() {
        let invalid = ProxySetting::Configured("://invalid-proxy".to_string());
        let http = ProxySetting::Configured("http://127.0.0.1:7890".to_string());
        let socks5 = ProxySetting::Configured("socks5://127.0.0.1:1080".to_string());
        assert!(UsageSegment::build_agent(invalid).is_none());
        assert!(UsageSegment::build_agent(http).is_some());
        assert!(UsageSegment::build_agent(socks5).is_some());
        assert!(UsageSegment::build_agent(ProxySetting::None).is_some());
    }

    #[test]
    fn backoff_scales_and_caps() {
        assert_eq!(UsageSegment::backoff_seconds(1, true), 60);
        assert_eq!(UsageSegment::backoff_seconds(2, true), 120);
        assert_eq!(UsageSegment::backoff_seconds(10, true), 900);
        assert_eq!(UsageSegment::backoff_seconds(1, false), 30);
        assert_eq!(UsageSegment::backoff_seconds(2, false), 60);
        assert_eq!(UsageSegment::backoff_seconds(10, false), 300);
    }

    #[test]
    fn legacy_cache_deserializes_cleanly() {
        let legacy = r#"{
            "five_hour_utilization": 18.5,
            "seven_day_utilization": 26.0,
            "resets_at": "2026-09-20T00:00:00Z",
            "cached_at": "2026-09-17T10:00:00Z"
        }"#;
        let cache: ApiUsageCache = serde_json::from_str(legacy).unwrap();
        assert_eq!(cache.five_hour_utilization, Some(18.5));
        assert_eq!(cache.seven_day_utilization, Some(26.0));
        assert_eq!(cache.consecutive_failures, 0);
        assert!(cache.retry_after.is_none());
    }

    #[test]
    fn with_options_overrides_defaults() {
        let mut opts = HashMap::new();
        opts.insert(
            "api_base_url".to_string(),
            serde_json::Value::String("https://proxy.example.com".to_string()),
        );
        opts.insert("cache_duration".to_string(), serde_json::Value::from(60));
        opts.insert("timeout".to_string(), serde_json::Value::from(5));

        let seg = UsageSegment::new().with_options(&opts);
        assert_eq!(seg.api_base_url, "https://proxy.example.com");
        assert_eq!(seg.cache_duration, 60);
        assert_eq!(seg.timeout, 5);
    }
}
