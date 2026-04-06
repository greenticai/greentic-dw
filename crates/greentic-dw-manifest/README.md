# greentic-dw-manifest

DW manifest wrapper and migration layer. This crate keeps the workspace-specific manifest
contract (`version`, `worker_version`, tenancy, locale, and embedded shared capability
declarations) while reusing the capability primitives from the local capability workspace crates.
