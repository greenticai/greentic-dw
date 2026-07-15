//! DW CLI and localized wizard flow.
//!
//! The `gtc worker` authoring pipeline (worker.rs) is intentionally absent on
//! the develop lane: `greentic-dw-authoring` pins research-lane crates that
//! cannot resolve under develop's 1.2.0-dev version range. The full worker
//! module is preserved on the `main` branch and in git history.

mod cli_types;
mod i18n;
mod wizard;
mod wizard_tests;

pub use cli_types::*;
pub use wizard::*;
