# greentic-dw-runtime

Runtime kernel for DW execution, including capability-aware bindings and provider-agnostic
state/memory access paths.

Use this crate with resolved capability output from bundle/setup tooling. The runtime can:

- accept `CapabilityResolution`-backed bindings from the versioned `greentic-cap` crates
- dispatch memory/state operations through a capability dispatcher
- keep task state/resume access provider-agnostic
