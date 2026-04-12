//! Canonical Digital Worker (DW) core contracts.
//!
//! This crate defines shared types for task envelopes, lifecycle states,
//! locale handling, tenant/team scope, template descriptors, and shared
//! source-reference models.

pub type TaskId = String;
pub type WorkerId = String;
pub type TenantId = String;
pub type TeamId = String;

mod app_model;
mod bundle_plan;
mod bundle_plan_tests;
mod composition;
mod composition_tests;
mod pack_spec;
mod pack_spec_tests;
mod provider_catalog;
mod provider_catalog_tests;
mod qa;
mod qa_tests;
mod resolver;
mod resolver_tests;
mod source_ref;
mod source_ref_tests;
mod starter_catalogs_tests;
mod task;
mod task_tests;
mod template;
mod template_catalog;
mod template_catalog_tests;
mod template_tests;

pub use app_model::*;
pub use bundle_plan::*;
pub use composition::*;
pub use pack_spec::*;
pub use provider_catalog::*;
pub use qa::*;
pub use resolver::*;
pub use source_ref::*;
pub use task::*;
pub use template::*;
pub use template_catalog::*;
