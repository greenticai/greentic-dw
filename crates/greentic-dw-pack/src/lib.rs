//! Hook/sub integration surfaces for Digital Worker runtime.

use greentic_cap_resolver::{CapabilityResolutionIssue, CapabilityResolutionReport};
use greentic_cap_schema::{
    CapabilityCompatibilityReport, CapabilitySchemaError, PackCapabilitySectionV1,
    check_pack_capability_compatibility, decode_pack_capability_section_from_cbor,
    encode_pack_capability_section_to_cbor, pack_capability_section_schema,
};
use greentic_cap_types::{CapabilityComponentDescriptor, CapabilityDeclaration};
use greentic_dw_core::{RuntimeEvent, RuntimeOperation};
use greentic_dw_types::TaskEnvelope;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookDecision {
    Continue,
    Block { reason: String },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HookError {
    #[error("operation blocked by control hook: {reason}")]
    Blocked { reason: String },
}

/// Control hook trait for policy enforcement around runtime operations.
pub trait ControlHook: Send + Sync {
    fn pre_operation(&self, envelope: &TaskEnvelope, operation: &RuntimeOperation) -> HookDecision;
    fn post_operation(&self, envelope: &TaskEnvelope, event: &RuntimeEvent);
}

/// Observer subscription trait for audit/telemetry style notifications.
pub trait ObserverSub: Send + Sync {
    fn on_operation(&self, event: &RuntimeEvent);
}

/// Creates a versioned pack capability section from a shared capability declaration.
pub fn pack_capabilities(declaration: CapabilityDeclaration) -> PackCapabilitySectionV1 {
    PackCapabilitySectionV1::new(declaration)
}

/// Decodes a pack capability section from CBOR bytes.
pub fn read_pack_capabilities_from_cbor(
    bytes: &[u8],
) -> Result<PackCapabilitySectionV1, CapabilitySchemaError> {
    decode_pack_capability_section_from_cbor(bytes)
}

/// Encodes a pack capability section to CBOR bytes.
pub fn write_pack_capabilities_to_cbor(
    section: &PackCapabilitySectionV1,
) -> Result<Vec<u8>, CapabilitySchemaError> {
    encode_pack_capability_section_to_cbor(section)
}

/// Validates a pack capability section against a provider self-description.
pub fn validate_pack_capabilities(
    section: &PackCapabilitySectionV1,
    component: &CapabilityComponentDescriptor,
) -> Result<Vec<CapabilityCompatibilityReport>, CapabilitySchemaError> {
    check_pack_capability_compatibility(section, component)
}

/// Returns unresolved capability request identifiers for bundle/setup UI surfaces.
pub fn unresolved_capability_request_ids(report: &CapabilityResolutionReport) -> Vec<String> {
    report
        .issues
        .iter()
        .filter_map(|issue| match issue {
            CapabilityResolutionIssue::UnresolvedRequest { request_id, .. } => {
                Some(request_id.clone())
            }
            _ => None,
        })
        .collect()
}

/// Returns the JSON schema for the shared pack capability section.
pub fn pack_capabilities_schema() -> serde_json::Value {
    serde_json::to_value(pack_capability_section_schema())
        .expect("capability pack schema serialization should succeed")
}

/// Re-export the shared capability data model so DW callers can stay on path dependencies.
pub use greentic_cap_schema::CapabilitySchemaError as PackCapabilityError;
pub use greentic_cap_schema::PackCapabilitySectionV1 as DwPackCapabilitySection;
pub use greentic_cap_schema::{
    binding_emission_from_binding, build_bundle_resolution_artifact, capability_binding_schema,
    capability_declaration_schema, capability_profile_schema, capability_resolution_schema,
    emitted_binding_count, emitted_bindings, target_from_resolution_report,
    target_with_reference_policies,
};
pub use greentic_cap_types::{
    CapabilityBinding, CapabilityBindingKind, CapabilityConsume, CapabilityConsumeMode,
    CapabilityId, CapabilityMetadata, CapabilityResolution,
};

/// Integration registry for hooks and observers.
#[derive(Default)]
pub struct PackIntegration {
    control_hooks: Vec<Box<dyn ControlHook>>,
    observer_subs: Vec<Box<dyn ObserverSub>>,
}

impl PackIntegration {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_control_hook(mut self, hook: impl ControlHook + 'static) -> Self {
        self.control_hooks.push(Box::new(hook));
        self
    }

    pub fn with_observer_sub(mut self, sub: impl ObserverSub + 'static) -> Self {
        self.observer_subs.push(Box::new(sub));
        self
    }

    pub fn run_pre_hooks(
        &self,
        envelope: &TaskEnvelope,
        operation: &RuntimeOperation,
    ) -> Result<(), HookError> {
        for hook in &self.control_hooks {
            match hook.pre_operation(envelope, operation) {
                HookDecision::Continue => {}
                HookDecision::Block { reason } => {
                    return Err(HookError::Blocked { reason });
                }
            }
        }

        Ok(())
    }

    pub fn run_post_hooks(&self, envelope: &TaskEnvelope, event: &RuntimeEvent) {
        for hook in &self.control_hooks {
            hook.post_operation(envelope, event);
        }
    }

    pub fn notify_observers(&self, event: &RuntimeEvent) {
        for observer in &self.observer_subs {
            observer.on_operation(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_cap_resolver::{CapabilityResolutionIssue, CapabilityResolutionReport};
    use greentic_cap_types::{
        CapabilityComponentDescriptor, CapabilityComponentOperation, CapabilityConsume,
        CapabilityDeclaration, CapabilityId, CapabilityOffer, CapabilityProviderOperationMap,
        CapabilityProviderRef, CapabilityRequirement,
    };
    use greentic_dw_core::RuntimeOperation;
    use greentic_dw_types::{
        LocaleContext, LocalePropagation, OutputLocaleGuidance, TaskLifecycleState, TenantScope,
        WorkerLocalePolicy,
    };
    use std::sync::{Arc, Mutex};

    fn sample_envelope() -> TaskEnvelope {
        TaskEnvelope {
            task_id: "task-1".to_string(),
            worker_id: "worker-1".to_string(),
            state: TaskLifecycleState::Created,
            scope: TenantScope {
                tenant: "tenant-a".to_string(),
                team: None,
            },
            locale: LocaleContext {
                worker_default_locale: "en-US".to_string(),
                requested_locale: None,
                human_locale: None,
                policy: WorkerLocalePolicy::WorkerDefault,
                propagation: LocalePropagation::CurrentTaskOnly,
                output: OutputLocaleGuidance::WorkerDefault,
            },
        }
    }

    struct BlockStartHook;

    impl ControlHook for BlockStartHook {
        fn pre_operation(
            &self,
            _envelope: &TaskEnvelope,
            operation: &RuntimeOperation,
        ) -> HookDecision {
            if matches!(operation, RuntimeOperation::Start) {
                HookDecision::Block {
                    reason: "start disabled by policy".to_string(),
                }
            } else {
                HookDecision::Continue
            }
        }

        fn post_operation(&self, _envelope: &TaskEnvelope, _event: &RuntimeEvent) {}
    }

    struct RecordingObserver {
        count: Arc<Mutex<u32>>,
    }

    impl ObserverSub for RecordingObserver {
        fn on_operation(&self, _event: &RuntimeEvent) {
            let mut count = self.count.lock().expect("lock count");
            *count += 1;
        }
    }

    #[test]
    fn pre_hook_can_block_operation() {
        let integration = PackIntegration::new().with_control_hook(BlockStartHook);
        let env = sample_envelope();

        let err = integration
            .run_pre_hooks(&env, &RuntimeOperation::Start)
            .expect_err("start should be blocked");

        assert_eq!(
            err,
            HookError::Blocked {
                reason: "start disabled by policy".to_string(),
            }
        );
    }

    #[test]
    fn observer_receives_event() {
        let count = Arc::new(Mutex::new(0));
        let integration = PackIntegration::new().with_observer_sub(RecordingObserver {
            count: Arc::clone(&count),
        });

        let event = RuntimeEvent {
            task_id: "task-1".to_string(),
            worker_id: "worker-1".to_string(),
            operation: RuntimeOperation::Start,
            from_state: TaskLifecycleState::Created,
            to_state: TaskLifecycleState::Running,
        };

        integration.notify_observers(&event);

        assert_eq!(*count.lock().expect("lock count"), 1);
    }

    fn capability_component() -> CapabilityComponentDescriptor {
        CapabilityComponentDescriptor {
            component_ref: "component:memory.redis".to_string(),
            version: "1.0.0".to_string(),
            operations: vec![
                CapabilityComponentOperation {
                    name: "memory.get".to_string(),
                    input_schema: serde_json::json!({"type": "object"}),
                    output_schema: serde_json::json!({"type": "string"}),
                },
                CapabilityComponentOperation {
                    name: "memory.put".to_string(),
                    input_schema: serde_json::json!({"type": "object"}),
                    output_schema: serde_json::json!({"type": "null"}),
                },
            ],
            capabilities: vec![CapabilityId::new("cap://dw.memory.short-term").expect("cap")],
            metadata: Default::default(),
        }
    }

    fn capability_declaration() -> CapabilityDeclaration {
        let mut declaration = CapabilityDeclaration::new();

        let mut offer = CapabilityOffer::new(
            "offer.short-term-memory",
            CapabilityId::new("cap://dw.memory.short-term").expect("cap"),
        );
        offer.provider = Some(CapabilityProviderRef {
            component_ref: "component:memory.redis".to_string(),
            operation: "memory.put".to_string(),
            operation_map: vec![
                CapabilityProviderOperationMap {
                    contract_operation: "get".to_string(),
                    component_operation: "memory.get".to_string(),
                    input_schema: serde_json::json!({"type": "object"}),
                    output_schema: serde_json::json!({"type": "string"}),
                },
                CapabilityProviderOperationMap {
                    contract_operation: "put".to_string(),
                    component_operation: "memory.put".to_string(),
                    input_schema: serde_json::json!({"type": "object"}),
                    output_schema: serde_json::json!({"type": "null"}),
                },
            ],
        });
        declaration.offers.push(offer);

        let mut requirement = CapabilityRequirement::new(
            "require.memory",
            CapabilityId::new("cap://dw.memory.short-term").expect("cap"),
        );
        requirement.description = Some("short term memory".to_string());
        declaration.requires.push(requirement);

        let mut consume = CapabilityConsume::new(
            "consume.memory",
            CapabilityId::new("cap://dw.memory.short-term").expect("cap"),
        );
        consume.description = Some("short term memory".to_string());
        declaration.consumes.push(consume);

        declaration
    }

    #[test]
    fn pack_capabilities_round_trip_and_validate() {
        let declaration = capability_declaration();
        let section = pack_capabilities(declaration.clone());
        let bytes = write_pack_capabilities_to_cbor(&section).expect("encode");
        let decoded = read_pack_capabilities_from_cbor(&bytes).expect("decode");

        assert_eq!(section, decoded);

        let reports =
            validate_pack_capabilities(&decoded, &capability_component()).expect("validate");
        assert_eq!(reports.len(), 1);
        assert!(reports[0].compatible);
    }

    #[test]
    fn pack_capabilities_schema_is_available() {
        let schema = pack_capabilities_schema();
        assert!(schema.is_object());
    }

    #[test]
    fn unresolved_capability_requests_are_surfaceable_for_setup() {
        let report = CapabilityResolutionReport {
            profile_id: Some("dw-default".to_string()),
            resolution: greentic_cap_types::CapabilityResolution::new(CapabilityDeclaration::new()),
            issues: vec![
                CapabilityResolutionIssue::UnresolvedRequest {
                    profile_id: Some("dw-default".to_string()),
                    request_id: "require.dw.memory".to_string(),
                    capability: CapabilityId::new("cap://dw.memory.short-term").expect("cap"),
                    request_kind: greentic_cap_types::CapabilityBindingKind::Requirement,
                },
                CapabilityResolutionIssue::AmbiguousOffer {
                    profile_id: Some("dw-default".to_string()),
                    request_id: "consume.dw.memory".to_string(),
                    capability: CapabilityId::new("cap://dw.memory.short-term").expect("cap"),
                    request_kind: greentic_cap_types::CapabilityBindingKind::Consume,
                    candidates: vec!["offer.a".to_string(), "offer.b".to_string()],
                    chosen: "offer.a".to_string(),
                },
            ],
        };

        let unresolved = unresolved_capability_request_ids(&report);
        assert_eq!(unresolved, vec!["require.dw.memory".to_string()]);
    }
}
