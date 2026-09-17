use ccometixline::config::{InputData, ModelConfig, ModelEntry};

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
