#[cfg(test)]
mod tests {
    use greentic_cap_types::CapabilityId;
    use greentic_dw_types::{
        DwAgentResolveRequest, DwCompositionDocument, DwCompositionResolveRequest,
        DwProviderCatalog, DwResolutionMode, TemplateCatalog,
    };
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn workspace_examples_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .canonicalize()
            .expect("workspace examples dir")
    }

    fn template_catalog() -> TemplateCatalog {
        TemplateCatalog::from_json_path(workspace_examples_dir().join("templates/catalog.json"))
            .expect("load template catalog")
    }

    fn provider_catalog() -> DwProviderCatalog {
        DwProviderCatalog::from_json_path(workspace_examples_dir().join("providers/catalog.json"))
            .expect("load provider catalog")
    }

    fn agent_request(template_id: &str, agent_id: &str) -> DwAgentResolveRequest {
        let catalog = template_catalog();
        DwAgentResolveRequest {
            agent_id: agent_id.to_string(),
            display_name: None,
            template: catalog
                .resolve_template(template_id)
                .expect("resolve starter template"),
            selected_template: catalog.find(template_id).cloned(),
            answers: BTreeMap::new(),
            provider_overrides: BTreeMap::new(),
        }
    }

    fn resolve(
        application_id: &str,
        display_name: &str,
        mode: DwResolutionMode,
        agents: Vec<DwAgentResolveRequest>,
        shared_provider_overrides: BTreeMap<CapabilityId, String>,
    ) -> DwCompositionDocument {
        DwCompositionResolveRequest {
            application_id: application_id.to_string(),
            display_name: display_name.to_string(),
            version: Some("0.5.0".to_string()),
            tenant: Some("tenant-starter".to_string()),
            tags: vec!["starter".to_string(), "e2e".to_string()],
            agents,
            shared_provider_overrides,
            mode: Some(mode),
        }
        .resolve(&provider_catalog())
        .expect("resolve composition")
    }

    fn capability_shape(document: &DwCompositionDocument) -> Vec<(String, Vec<String>)> {
        let mut shape = document
            .agents
            .iter()
            .map(|agent| {
                let mut capabilities = agent
                    .capability_bindings
                    .iter()
                    .map(|binding| binding.capability_id.as_str().to_string())
                    .collect::<Vec<_>>();
                capabilities.sort();
                (agent.agent_id.clone(), capabilities)
            })
            .collect::<Vec<_>>();
        shape.sort_by(|left, right| left.0.cmp(&right.0));
        shape
    }

    #[test]
    fn starter_single_agent_default_and_personalised_flows_share_shape() {
        let default_document = resolve(
            "dw.app.support-default",
            "Support Default",
            DwResolutionMode::Recommended,
            vec![agent_request("dw.support-assistant", "support-assistant")],
            BTreeMap::new(),
        );
        let personalised_document = resolve(
            "dw.app.support-personalised",
            "Support Personalised",
            DwResolutionMode::ReviewAll,
            vec![agent_request("dw.support-assistant", "support-assistant")],
            BTreeMap::new(),
        );

        assert_eq!(default_document.agents.len(), 1);
        assert_eq!(personalised_document.agents.len(), 1);
        assert_eq!(
            capability_shape(&default_document),
            capability_shape(&personalised_document)
        );
        assert!(default_document.unresolved_setup_items.is_empty());
        assert!(personalised_document.unresolved_setup_items.is_empty());

        let default_blocks = &default_document.agents[0]
            .behavior_config
            .enabled_question_block_ids;
        let personalised_blocks = &personalised_document.agents[0]
            .behavior_config
            .enabled_question_block_ids;
        assert_eq!(
            default_blocks,
            &["dw.core.identity", "dw.support.behavior"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
        assert!(personalised_blocks.len() > default_blocks.len());
        assert!(personalised_blocks.contains(&"provider.llm.chat.openai".to_string()));
    }

    #[test]
    fn starter_single_agent_personalised_flow_applies_provider_override() {
        let mut agent = agent_request("dw.support-assistant", "support-assistant");
        agent.provider_overrides.insert(
            CapabilityId::new("cap://llm/chat").expect("capability id"),
            "provider.llm.openai.chat-lite".to_string(),
        );

        let document = resolve(
            "dw.app.support-override",
            "Support Override",
            DwResolutionMode::ReviewAll,
            vec![agent],
            BTreeMap::new(),
        );
        let binding = document.agents[0]
            .capability_bindings
            .iter()
            .find(|binding| binding.capability_id.as_str() == "cap://llm/chat")
            .expect("llm binding");

        assert_eq!(
            binding.provider_binding.provider_id,
            "provider.llm.openai.chat-lite"
        );

        let pack_spec = document
            .to_application_pack_spec()
            .expect("materialize app pack");
        assert_eq!(pack_spec.agents.len(), 1);
        assert!(pack_spec.requirements.iter().any(|requirement| {
            requirement.provider_id.as_deref() == Some("provider.llm.openai.chat-lite")
        }));

        let bundle_plan = document.to_bundle_plan().expect("generate bundle plan");
        assert!(bundle_plan.provider_packs.iter().any(|pack| {
            pack.provider_id == "provider.llm.openai.chat-lite"
                && pack.pack_id == "provider.llm.openai.chat-lite"
        }));
    }

    #[test]
    fn starter_multi_agent_default_flow_generates_shared_pack_and_bundle_outputs() {
        let document = resolve(
            "dw.app.support-approval",
            "Support Approval",
            DwResolutionMode::Recommended,
            vec![
                agent_request("dw.support-assistant", "support-assistant"),
                agent_request("dw.approval-worker", "approval-worker"),
            ],
            BTreeMap::new(),
        );

        assert_eq!(document.agents.len(), 2);
        assert!(document.output_plan.supports_multi_agent_app_pack);

        let observer_dependency = document
            .shared_pack_dependencies
            .iter()
            .find(|dependency| dependency.pack_id == "provider.observer.audit.basic")
            .expect("shared observer dependency");
        let mut applies_to_agents = observer_dependency.applies_to_agents.clone();
        applies_to_agents.sort();
        assert_eq!(
            applies_to_agents,
            vec![
                "approval-worker".to_string(),
                "support-assistant".to_string()
            ]
        );

        let pack_spec = document
            .to_application_pack_spec()
            .expect("materialize app pack");
        assert!(pack_spec.metadata.multi_agent);
        assert_eq!(pack_spec.agents.len(), 2);
        assert!(pack_spec.generated_configs.iter().any(|asset| {
            asset.asset_id == "generated.config.application" && asset.applies_to_agents.len() == 2
        }));

        let bundle_plan = document.to_bundle_plan().expect("generate bundle plan");
        assert!(bundle_plan.multi_agent);
        assert_eq!(bundle_plan.generated_app_pack.applies_to_agents.len(), 2);
        assert!(bundle_plan.provider_packs.iter().any(|pack| {
            pack.provider_id == "provider.observer.audit.basic"
                && pack.applies_to_agents
                    == vec![
                        "approval-worker".to_string(),
                        "support-assistant".to_string(),
                    ]
        }));
        assert_eq!(bundle_plan.support_packs.len(), 2);
    }

    #[test]
    fn starter_multi_agent_personalised_flow_preserves_shape_with_agent_overrides() {
        let default_document = resolve(
            "dw.app.multi-default",
            "Multi Default",
            DwResolutionMode::Recommended,
            vec![
                agent_request("dw.support-assistant", "support-assistant"),
                agent_request("dw.approval-worker", "approval-worker"),
            ],
            BTreeMap::new(),
        );

        let support_agent = agent_request("dw.support-assistant", "support-assistant");
        let mut approval_agent = agent_request("dw.approval-worker", "approval-worker");
        approval_agent.provider_overrides.insert(
            CapabilityId::new("cap://llm/chat").expect("capability id"),
            "provider.llm.openai.chat-lite".to_string(),
        );

        let personalised_document = resolve(
            "dw.app.multi-personalised",
            "Multi Personalised",
            DwResolutionMode::ReviewAll,
            vec![support_agent, approval_agent],
            BTreeMap::new(),
        );

        assert_eq!(
            capability_shape(&default_document),
            capability_shape(&personalised_document)
        );
        assert!(
            personalised_document
                .output_plan
                .supports_multi_agent_app_pack
        );
        assert!(
            personalised_document
                .agents
                .iter()
                .all(|agent| { agent.behavior_config.enabled_question_block_ids.len() >= 3 })
        );

        let support_llm = personalised_document
            .agents
            .iter()
            .find(|agent| agent.agent_id == "support-assistant")
            .and_then(|agent| {
                agent
                    .capability_bindings
                    .iter()
                    .find(|binding| binding.capability_id.as_str() == "cap://llm/chat")
            })
            .expect("support llm binding");
        let approval_llm = personalised_document
            .agents
            .iter()
            .find(|agent| agent.agent_id == "approval-worker")
            .and_then(|agent| {
                agent
                    .capability_bindings
                    .iter()
                    .find(|binding| binding.capability_id.as_str() == "cap://llm/chat")
            })
            .expect("approval llm binding");

        assert_eq!(
            support_llm.provider_binding.provider_id,
            "provider.llm.openai.chat"
        );
        assert_eq!(
            approval_llm.provider_binding.provider_id,
            "provider.llm.openai.chat-lite"
        );

        let bundle_plan = personalised_document
            .to_bundle_plan()
            .expect("generate bundle plan");
        assert!(bundle_plan.provider_packs.iter().any(|pack| {
            pack.provider_id == "provider.llm.openai.chat"
                && pack.applies_to_agents == vec!["support-assistant".to_string()]
        }));
        assert!(bundle_plan.provider_packs.iter().any(|pack| {
            pack.provider_id == "provider.llm.openai.chat-lite"
                && pack.applies_to_agents == vec!["approval-worker".to_string()]
        }));
    }
}
