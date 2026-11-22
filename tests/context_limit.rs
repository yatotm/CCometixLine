use ccometixline::config::{InputData, ModelConfig, ModelEntry};
use ccometixline::core::segments::{
    ContextWindowSegment, ModelSegment, OutputStyleSegment, Segment, SessionSegment,
};

fn parse(json: &str) -> InputData {
    serde_json::from_str(json).expect("valid statusline input")
}

const BASE_FIELDS: &str = r#""workspace":{"current_dir":"/tmp"},"transcript_path":"/tmp/t.jsonl""#;

// --- InputData: native context window field (new Claude Code) ---------------

#[test]
fn native_limit_is_read_from_context_window() {
    let input = parse(&format!(
        r#"{{"model":{{"id":"claude-sonnet-5","display_name":"Sonnet 5"}},{BASE_FIELDS},
            "version":"2.1.263","context_window":{{"context_window_size":1000000,"used_percentage":8}}}}"#
    ));
    assert_eq!(input.native_context_limit(), Some(1_000_000));
}

#[test]
fn native_limit_is_none_on_old_claude_code_payloads() {
    let input = parse(&format!(
        r#"{{"model":{{"id":"claude-sonnet-4-5","display_name":"Sonnet 4.5"}},{BASE_FIELDS}}}"#
    ));
    assert_eq!(input.native_context_limit(), None);
}

#[test]
fn native_limit_ignores_zero_or_missing_size() {
    let zero = parse(&format!(
        r#"{{"model":{{"id":"claude-opus-5","display_name":"Opus 5"}},{BASE_FIELDS},"context_window":{{"context_window_size":0}}}}"#
    ));
    assert_eq!(zero.native_context_limit(), None);

    let missing = parse(&format!(
        r#"{{"model":{{"id":"claude-opus-5","display_name":"Opus 5"}},{BASE_FIELDS},"context_window":{{"used_percentage":null}}}}"#
    ));
    assert_eq!(missing.native_context_limit(), None);
}

// --- InputData: model field as string or object (#118) ----------------------

#[test]
fn model_accepts_bare_string() {
    let input = parse(&format!(r#"{{"model":"claude-opus-4-6",{BASE_FIELDS}}}"#));
    assert_eq!(input.model.id, "claude-opus-4-6");
    assert_eq!(input.model.display_name, "");
}

#[test]
fn model_accepts_object_without_display_name() {
    let input = parse(&format!(
        r#"{{"model":{{"id":"claude-opus-4-6"}},{BASE_FIELDS}}}"#
    ));
    assert_eq!(input.model.id, "claude-opus-4-6");
    assert_eq!(input.model.display_name, "");
}

#[test]
fn model_accepts_full_object() {
    let input = parse(&format!(
        r#"{{"model":{{"id":"claude-opus-4-6[1m]","display_name":"Opus 4.6"}},{BASE_FIELDS}}}"#
    ));
    assert_eq!(input.model.id, "claude-opus-4-6[1m]");
    assert_eq!(input.model.display_name, "Opus 4.6");
}

// --- ModelConfig: context limit priority -------------------------------------

fn config() -> ModelConfig {
    ModelConfig::default()
}

#[test]
fn old_claude_code_keeps_suffix_based_logic() {
    let cfg = config();
    assert_eq!(
        cfg.get_context_limit("claude-sonnet-4-5-20250929", None),
        200_000
    );
    assert_eq!(
        cfg.get_context_limit("claude-sonnet-4-5-20250929[1m]", None),
        1_000_000
    );
    assert_eq!(cfg.get_context_limit("claude-opus-5", None), 200_000);
    assert_eq!(cfg.get_context_limit("claude-opus-5[1m]", None), 1_000_000);
}

#[test]
fn new_claude_code_native_size_enables_1m_without_suffix() {
    let cfg = config();
    assert_eq!(
        cfg.get_context_limit("claude-sonnet-5", Some(1_000_000)),
        1_000_000
    );
    assert_eq!(
        cfg.get_context_limit("claude-opus-5", Some(1_000_000)),
        1_000_000
    );
    // and a plain 200k session stays 200k
    assert_eq!(
        cfg.get_context_limit("claude-opus-5", Some(200_000)),
        200_000
    );
}

#[test]
fn suffix_modifier_beats_native_size() {
    let cfg = config();
    assert_eq!(
        cfg.get_context_limit("claude-opus-5[1m]", Some(200_000)),
        1_000_000
    );
}

#[test]
fn explicit_model_entry_beats_native_size() {
    let mut cfg = config();
    cfg.model_entries.insert(
        0,
        ModelEntry {
            pattern: "my-proxy-model".to_string(),
            display_name: "Proxy".to_string(),
            context_limit: 128_000,
        },
    );
    assert_eq!(
        cfg.get_context_limit("my-proxy-model-v2", Some(1_000_000)),
        128_000
    );
    // built-in third-party entries behave the same way
    assert_eq!(cfg.get_context_limit("glm-4.5", Some(1_000_000)), 128_000);
}

#[test]
fn fable_and_mythos_default_to_1m_even_without_native_size() {
    let cfg = config();
    assert_eq!(cfg.get_context_limit("claude-fable-5-1", None), 1_000_000);
    assert_eq!(cfg.get_context_limit("claude-fable-5", None), 1_000_000);
    assert_eq!(
        cfg.get_context_limit("claude-mythos-5-1-20260901", None),
        1_000_000
    );
    assert_eq!(
        cfg.get_display_name("claude-fable-5-1").as_deref(),
        Some("Fable 5.1")
    );
    assert_eq!(
        cfg.get_display_name("claude-mythos-5").as_deref(),
        Some("Mythos 5")
    );
}

#[test]
fn unknown_model_falls_back_to_native_then_default() {
    let cfg = config();
    assert_eq!(
        cfg.get_context_limit("some-unknown-model", Some(400_000)),
        400_000
    );
    assert_eq!(cfg.get_context_limit("some-unknown-model", None), 200_000);
    assert_eq!(cfg.get_display_name("some-unknown-model"), None);
}

#[test]
fn display_name_and_suffix_are_unaffected_by_native_size() {
    let cfg = config();
    assert_eq!(
        cfg.get_display_name("claude-opus-5[1m]").as_deref(),
        Some("Opus 5 1M")
    );
    assert_eq!(
        cfg.get_display_suffix("claude-opus-5[1m]").as_deref(),
        Some(" 1M")
    );
    assert_eq!(cfg.get_display_suffix("claude-opus-5"), None);
}

// --- InputData: resilient deserialization ------------------------------------

#[test]
fn input_data_tolerates_null_and_missing_optional_fields() {
    let input = parse(
        r#"{
            "model": null,
            "workspace": null,
            "cwd": "/tmp/fallback-dir/",
            "transcript_path": null,
            "version": "2.1.280",
            "output_style": { "name": null }
        }"#,
    );
    assert_eq!(input.model.id, "");
    assert_eq!(input.current_dir(), "/tmp/fallback-dir/");
    assert_eq!(input.transcript_path, "");
    assert_eq!(input.version.as_deref(), Some("2.1.280"));
    assert_eq!(input.output_style.as_ref().unwrap().name, "");
}

// --- ContextWindowSegment: native current_usage & transcript fallback --------

#[test]
fn context_window_uses_native_current_usage_and_percentage() {
    let input = parse(&format!(
        r#"{{
            "model": {{"id": "claude-opus-5", "display_name": "Opus 5"}},
            {BASE_FIELDS},
            "context_window": {{
                "context_window_size": 1000000,
                "used_percentage": 22.5,
                "current_usage": {{
                    "input_tokens": 5000,
                    "output_tokens": 9000,
                    "cache_creation_input_tokens": 20000,
                    "cache_read_input_tokens": 200000
                }}
            }}
        }}"#
    ));

    let seg = ContextWindowSegment::new().with_model_config(config());
    let data = seg.collect(&input).expect("context window data");
    // active_context_tokens = 5000 + 20000 + 200000 = 225000 (excludes output_tokens)
    assert_eq!(data.primary, "22.5% · 225k tokens");
}

#[test]
fn context_window_recalculates_percentage_when_model_entry_overrides_limit() {
    let mut cfg = config();
    cfg.model_entries.insert(
        0,
        ModelEntry {
            pattern: "proxy-model".to_string(),
            display_name: "Proxy".to_string(),
            context_limit: 100_000,
        },
    );

    let input = parse(&format!(
        r#"{{
            "model": {{"id": "proxy-model"}},
            {BASE_FIELDS},
            "context_window": {{
                "context_window_size": 200000,
                "used_percentage": 25.0,
                "current_usage": {{
                    "input_tokens": 50000,
                    "output_tokens": 1000
                }}
            }}
        }}"#
    ));

    let seg = ContextWindowSegment::new().with_model_config(cfg);
    let data = seg.collect(&input).expect("context window data");
    assert_eq!(data.primary, "50% · 50k tokens");
}

#[test]
fn context_window_uses_used_percentage_when_current_usage_missing() {
    let input = parse(&format!(
        r#"{{
            "model": {{"id": "claude-opus-5"}},
            {BASE_FIELDS},
            "context_window": {{
                "context_window_size": 200000,
                "used_percentage": 15.0
            }}
        }}"#
    ));

    let seg = ContextWindowSegment::new().with_model_config(config());
    let data = seg.collect(&input).expect("context window data");
    assert_eq!(data.primary, "15% · 30k tokens");
}

#[test]
fn context_window_does_not_leak_previous_session_history_on_fresh_session() {
    let dir = std::env::temp_dir().join(format!("ccline-cw-fresh-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // Write an older session file in the same project directory
    std::fs::write(
        dir.join("older-session.jsonl"),
        r#"{"type":"assistant","message":{"usage":{"input_tokens":50000,"output_tokens":1000}}}"#,
    )
    .unwrap();

    let missing_transcript = dir.join("brand-new-session.jsonl");
    let input = parse(&format!(
        r#"{{
            "model": {{"id": "claude-opus-5"}},
            "workspace": {{"current_dir": "/tmp"}},
            "transcript_path": {},
            "context_window": {{"context_window_size": 200000, "used_percentage": null, "current_usage": null}}
        }}"#,
        serde_json::to_string(&missing_transcript.to_string_lossy()).unwrap()
    ));

    let seg = ContextWindowSegment::new().with_model_config(config());
    let data = seg.collect(&input).expect("context window data");
    assert_eq!(data.primary, "- · - tokens");

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn context_window_parses_large_transcript_tail_for_legacy_cli() {
    let dir = std::env::temp_dir().join(format!("ccline-cw-tail-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let transcript = dir.join("large.jsonl");
    let mut content = String::new();
    let pad = "x".repeat(4096);
    for _ in 0..80 {
        content.push_str(&format!(r#"{{"type":"user","summary":"{}"}}"#, pad));
        content.push('\n');
    }
    content.push_str(
        r#"{"type":"assistant","message":{"usage":{"input_tokens":10000,"cache_read_input_tokens":30000,"output_tokens":0}}}"#,
    );
    content.push('\n');
    std::fs::write(&transcript, content).unwrap();

    let input = parse(&format!(
        r#"{{
            "model": {{"id": "claude-sonnet-4-5"}},
            "workspace": {{"current_dir": "/tmp"}},
            "transcript_path": {}
        }}"#,
        serde_json::to_string(&transcript.to_string_lossy()).unwrap()
    ));

    let seg = ContextWindowSegment::new().with_model_config(config());
    let data = seg.collect(&input).expect("context window data");
    assert_eq!(data.primary, "20% · 40k tokens");

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn model_segment_does_not_duplicate_1m_suffix() {
    let input = parse(&format!(
        r#"{{
            "model": {{"id": "custom-model", "display_name": "Custom (1M context)"}},
            {BASE_FIELDS},
            "context_window": {{"context_window_size": 1000000}}
        }}"#
    ));
    let seg = ModelSegment::new().with_model_config(config());
    let data = seg.collect(&input).expect("model data");
    assert_eq!(data.primary, "Custom (1M context)");
}

#[test]
fn model_regex_recognizes_bedrock_vertex_and_dotted_versions() {
    let cfg = config();
    let bedrock = cfg.get_display_name("us.anthropic.claude-sonnet-4-5-20250929-v1:0");
    assert_eq!(bedrock.as_deref(), Some("Sonnet 4.5"));
    assert_eq!(
        cfg.get_display_name("claude-sonnet-5@20250929").as_deref(),
        Some("Sonnet 5")
    );
    assert_eq!(
        cfg.get_display_name("anthropic/claude-opus-4.1").as_deref(),
        Some("Opus 4.1")
    );
}

#[test]
fn lenient_deserialization_drops_only_malformed_fields() {
    let input = parse(
        r#"{
            "model": {"id": "claude-opus-4-6"},
            "cost": {"total_cost_usd": 1.25, "total_duration_ms": 61500.7},
            "prompt_cache": "unexpected-shape",
            "vim": 42
        }"#,
    );
    assert_eq!(input.model.id, "claude-opus-4-6");
    assert_eq!(input.cost.as_ref().unwrap().total_cost_usd, Some(1.25));
    assert!(input.prompt_cache.is_none());
    assert!(input.vim.is_none());
}

#[test]
fn model_segment_shows_effort_fast_mode_and_agent() {
    let input = parse(&format!(
        r#"{{
            "model": {{"id": "claude-opus-4-6"}},
            {BASE_FIELDS},
            "effort": {{"level": "xhigh"}},
            "thinking": {{"enabled": true}},
            "fast_mode": true,
            "agent": {{"name": "Explore"}}
        }}"#
    ));
    let seg = ModelSegment::new().with_model_config(config());
    let data = seg.collect(&input).expect("model data");
    assert_eq!(data.primary, "Opus 4.6");
    assert_eq!(data.secondary, "· xHigh · Fast @Explore");

    // Thinking explicitly disabled hides effort
    let no_think = parse(&format!(
        r#"{{
            "model": {{"id": "claude-opus-4-6"}},
            {BASE_FIELDS},
            "effort": {{"level": "high"}},
            "thinking": {{"enabled": false}}
        }}"#
    ));
    let data = seg.collect(&no_think).expect("model data");
    assert_eq!(data.secondary, "");
}

#[test]
fn context_window_sets_severity_thresholds() {
    let warn_input = parse(&format!(
        r#"{{
            "model": {{"id": "claude-opus-4-6"}},
            {BASE_FIELDS},
            "context_window": {{"context_window_size": 200000, "used_percentage": 65.0}}
        }}"#
    ));
    let crit_input = parse(&format!(
        r#"{{
            "model": {{"id": "claude-opus-4-6"}},
            {BASE_FIELDS},
            "context_window": {{"context_window_size": 200000, "used_percentage": 85.0}}
        }}"#
    ));
    let seg = ContextWindowSegment::new().with_model_config(config());
    let warn_data = seg.collect(&warn_input).unwrap();
    let crit_data = seg.collect(&crit_input).unwrap();
    assert_eq!(
        warn_data.metadata.get("severity").map(String::as_str),
        Some("warn")
    );
    assert_eq!(
        crit_data.metadata.get("severity").map(String::as_str),
        Some("crit")
    );
}

#[test]
fn session_and_output_style_show_session_name_float_duration_and_vim_mode() {
    let input = parse(
        r#"{
            "session_name": "auth-refactor",
            "cost": {"total_duration_ms": 125400.5, "total_lines_added": 10, "total_lines_removed": 2},
            "output_style": {"name": "default"},
            "vim": {"mode": "NORMAL"}
        }"#,
    );
    let session = SessionSegment::new().collect(&input).expect("session data");
    assert_eq!(session.primary, "auth-refactor · 2m5s");

    let style = OutputStyleSegment::new().collect(&input).expect("output style");
    assert_eq!(style.primary, "default");
    assert_eq!(style.secondary, "[NORMAL]");
}
