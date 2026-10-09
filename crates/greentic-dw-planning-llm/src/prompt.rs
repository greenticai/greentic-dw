//! Prompt builders and JSON extractor for the LLM-backed planning methods.
//!
//! Each `system_for_*` function returns a system prompt that instructs the
//! model to respond with ONLY JSON matching a specific schema.  Each
//! `user_for_*` function serializes the request fields into a readable prompt.
//!
//! [`extract_json`] strips markdown code fences and surrounding prose so the
//! raw JSON object/array can be parsed directly.

use greentic_dw_planning::{
    CreatePlanRequest, NextActionsRequest, PlanDocument, PlanRevision, PlannedAction,
    RevisePlanRequest,
};

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

/// System prompt for `next_actions`: asks the model for a JSON array of
/// [`PlannedAction`] values.
pub fn system_for_next_actions() -> String {
    format!(
        "You are a planning assistant. Your task is to examine the current plan and determine \
which steps should be executed next. Respond with ONLY a JSON array matching this schema \
(no prose, no markdown fences, no explanation):\n\n{}",
        json_schema_for::<Vec<PlannedAction>>()
    )
}

/// System prompt for `create_plan`: asks the model for a JSON [`PlanDocument`].
pub fn system_for_create_plan() -> String {
    format!(
        "You are a planning assistant. Your task is to create a structured execution plan \
for the given goal. Respond with ONLY a JSON object matching this schema \
(no prose, no markdown fences, no explanation):\n\n{}",
        json_schema_for::<PlanDocument>()
    )
}

/// System prompt for `revise_plan`: asks the model for a JSON [`PlanRevision`].
pub fn system_for_revise_plan() -> String {
    format!(
        "You are a planning assistant. Your task is to revise an existing plan given a reason \
and optional context. Respond with ONLY a JSON object matching this schema \
(no prose, no markdown fences, no explanation):\n\n{}",
        json_schema_for::<PlanRevision>()
    )
}

// ---------------------------------------------------------------------------
// User prompts
// ---------------------------------------------------------------------------

/// Build the user prompt for `next_actions`.
pub fn user_for_next_actions(req: &NextActionsRequest) -> String {
    let plan_json = serde_json::to_string_pretty(&req.plan).unwrap_or_else(|_| "{}".into());
    let context_block = req
        .context
        .as_deref()
        .map(|ctx| format!("\n\nContext:\n{ctx}"))
        .unwrap_or_default();

    format!("Current plan:\n{plan_json}{context_block}")
}

/// Build the user prompt for `create_plan`.
pub fn user_for_create_plan(req: &CreatePlanRequest) -> String {
    let req_json = serde_json::to_string_pretty(req).unwrap_or_else(|_| "{}".into());
    format!("Plan request:\n{req_json}")
}

/// Build the user prompt for `revise_plan`.
pub fn user_for_revise_plan(req: &RevisePlanRequest) -> String {
    let plan_json = serde_json::to_string_pretty(&req.plan).unwrap_or_else(|_| "{}".into());
    let context_block = req
        .context
        .as_deref()
        .map(|ctx| format!("\n\nContext:\n{ctx}"))
        .unwrap_or_default();

    format!(
        "Reason for revision: {}\n\nCurrent plan:\n{plan_json}{context_block}",
        req.reason
    )
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
