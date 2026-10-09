//! The designer's deep-worker settings, as they arrive on a dispatch.
//!
//! The designer serialises its `DeepWorkerConfig` (camelCase) into the
//! `operala.call` node input under `deep_worker`. `planningModel` is accepted and
//! ignored: the runner builds one LLM per node, so a per-phase model has nowhere
//! to go yet (spec §2).

use anyhow::{Context, Result};
use greentic_dw_runtime::DEFAULT_MAX_ITERATIONS;
use serde::Deserialize;
use serde_json::Value;

/// Key under which the designer puts the settings in the dispatch input.
pub(crate) const DEEP_WORKER_KEY: &str = "deep_worker";

/// The designer's `DeepWorkerConfig::default().iteration_budget`.
const DESIGNER_DEFAULT_ITERATION_BUDGET: u32 = 8;

/// Settings as sent. Every field is optional so an older or partial config
/// still decodes; resolution into concrete values is [`ResolvedSettings`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepWorkerSettings {
    #[serde(default)]
    pub iteration_budget: Option<u32>,
    #[serde(default)]
    pub reflection: Option<bool>,
    #[serde(default)]
    pub delegation: Option<bool>,
}

/// Read `input.deep_worker`. Absent or `null` is `Ok(None)` (today's
/// behaviour); a present-but-malformed value is an error, because falling back
/// would silently run a 64-iteration loop the operator capped lower.
pub(crate) fn settings_from_input(input: &Value) -> Result<Option<DeepWorkerSettings>> {
    match input.get(DEEP_WORKER_KEY) {
        None | Some(Value::Null) => Ok(None),
        Some(raw) => serde_json::from_value::<DeepWorkerSettings>(raw.clone())
            .map(Some)
            .context("invalid `deep_worker` settings in the operala dispatch input"),
    }
}

/// Concrete knobs for one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResolvedSettings {
    pub max_iterations: usize,
    pub reflection: bool,
    pub delegation: bool,
}

impl ResolvedSettings {
    /// `None` keeps every caller predating the settings on the old behaviour
    /// (64 iterations, reflection and delegation on). `Some` means a designer
    /// config applies, so a missing field takes the DESIGNER's default
    /// (budget 8, reflection off, delegation off) — the values its composer shows.
    pub(crate) fn resolve(settings: Option<&DeepWorkerSettings>) -> Self {
        let Some(settings) = settings else {
            return Self {
                max_iterations: DEFAULT_MAX_ITERATIONS,
                reflection: true,
                delegation: true,
            };
        };
        let budget = settings
            .iteration_budget
            .unwrap_or(DESIGNER_DEFAULT_ITERATION_BUDGET)
            .max(1);
        Self {
            max_iterations: usize::try_from(budget).unwrap_or(DEFAULT_MAX_ITERATIONS),
            reflection: settings.reflection.unwrap_or(false),
            delegation: settings.delegation.unwrap_or(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn absent_or_null_deep_worker_is_none() {
        assert_eq!(
            settings_from_input(&json!({"goal": "g"})).expect("ok"),
            None
        );
        assert_eq!(
            settings_from_input(&json!({"deep_worker": null})).expect("ok"),
            None
        );
    }

    #[test]
    fn designer_config_decodes_camel_case_and_ignores_planning_model() {
        let settings = settings_from_input(&json!({
            "deep_worker": {
                "iterationBudget": 3,
                "reflection": true,
                "delegation": false,
                "planningModel": {"providerId": "openai", "model": "gpt-4o"}
            }
        }))
        .expect("ok")
        .expect("present");
        assert_eq!(
            settings,
            DeepWorkerSettings {
                iteration_budget: Some(3),
                reflection: Some(true),
                delegation: Some(false),
            }
        );
    }

    #[test]
    fn malformed_deep_worker_is_an_error_not_a_silent_default() {
        let err = settings_from_input(&json!({"deep_worker": {"iterationBudget": "eight"}}));
        assert!(err.is_err(), "a malformed budget must not fall back to 64");
    }

    #[test]
    fn absent_settings_resolve_to_todays_behaviour() {
        assert_eq!(
            ResolvedSettings::resolve(None),
            ResolvedSettings {
                max_iterations: 64,
                reflection: true,
                delegation: true,
            }
        );
    }

    #[test]
    fn present_settings_with_missing_fields_take_the_designer_defaults() {
        assert_eq!(
            ResolvedSettings::resolve(Some(&DeepWorkerSettings::default())),
            ResolvedSettings {
                max_iterations: 8,
                reflection: false,
                delegation: false,
            }
        );
    }

    #[test]
    fn a_zero_budget_is_clamped_to_one_iteration() {
        let settings = DeepWorkerSettings {
            iteration_budget: Some(0),
            reflection: Some(true),
            delegation: Some(true),
        };
        assert_eq!(ResolvedSettings::resolve(Some(&settings)).max_iterations, 1);
    }
}
