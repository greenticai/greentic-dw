//! Prompt builders and JSON extractor for the LLM-backed delegation methods.
//!
//! [`system_for_choose_delegate`] returns a system prompt that instructs the
//! model to respond with ONLY JSON matching the [`DelegationDecision`] schema.
//! [`user_for_choose_delegate`] serializes the request into a readable prompt.
//!
//! [`extract_json`] strips markdown code fences and surrounding prose so the
//! raw JSON object/array can be parsed directly.

use greentic_dw_delegation::{DelegationDecision, DelegationRequest};

// ---------------------------------------------------------------------------
// JSON extraction
// ---------------------------------------------------------------------------

/// Strip markdown ```json … ``` fences and leading/trailing prose from a
/// model reply, returning a slice that starts at the first `{` or `[` and
/// ends at the last `}` or `]`.
///
/// If neither delimiter is found the input is returned unchanged so the
/// downstream JSON parser can produce a meaningful error.
pub fn extract_json(reply: &str) -> &str {
    let start = reply.find(['{', '[']).unwrap_or(0);

    let end = reply
        .rfind(['}', ']'])
        .map(|i| i + 1)
        .unwrap_or(reply.len());

    if start < end {
        &reply[start..end]
    } else {
        reply
    }
}

// ---------------------------------------------------------------------------
// Schema helpers
// ---------------------------------------------------------------------------

/// Render a JSON Schema for the given type via schemars, falling back to an
/// empty object schema if serialization fails.
fn json_schema_for<T: schemars::JsonSchema>() -> String {
    serde_json::to_string_pretty(&schemars::schema_for!(T)).unwrap_or_else(|_| "{}".into())
}

// ---------------------------------------------------------------------------
// System prompts
// ---------------------------------------------------------------------------

/// System prompt for `choose_delegate`: asks the model to decide whether and
/// how to delegate the goal to sub-agents.
pub fn system_for_choose_delegate() -> String {
    format!(
        "You are a delegation strategist in a deep-worker system. Decide whether and how to \
delegate the goal to sub-agents. Respond with ONLY a JSON object matching this schema (no prose, \
no markdown fences):\n\n{}",
        json_schema_for::<DelegationDecision>()
    )
}

// ---------------------------------------------------------------------------
// User prompts
// ---------------------------------------------------------------------------

/// Build the user prompt for `choose_delegate`.
pub fn user_for_choose_delegate(req: &DelegationRequest) -> String {
    serde_json::to_string_pretty(req).unwrap_or_else(|_| "{}".into())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_plain_object() {
        let input = r#"{"key":"value"}"#;
        assert_eq!(extract_json(input), r#"{"key":"value"}"#);
    }

    #[test]
    fn extract_json_strips_fences() {
        let input = "```json\n{\"key\":\"value\"}\n```";
        assert_eq!(extract_json(input), r#"{"key":"value"}"#);
    }

    #[test]
    fn extract_json_strips_prose() {
        let input = "Here is the result:\n{\"key\":\"value\"}\nEnd.";
        assert_eq!(extract_json(input), r#"{"key":"value"}"#);
    }

    #[test]
    fn extract_json_array() {
        let input = "Result: [1, 2, 3]";
        assert_eq!(extract_json(input), "[1, 2, 3]");
    }

    #[test]
    fn extract_json_no_delimiters_returns_input() {
        let input = "not json at all";
        assert_eq!(extract_json(input), "not json at all");
    }

    #[test]
    fn extract_json_empty_input() {
        assert_eq!(extract_json(""), "");
    }
}
