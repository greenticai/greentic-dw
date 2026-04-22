use crate::cli_types::{
    AgentAnswerDocument, AnswerDocument, CONTRACT_VERSION, Cli, CliError, Command, WizardArgs,
    WizardOutput,
};
use crate::i18n::{MsgKey, prompt};
use clap::Parser;
use greentic_cap_types::CapabilityDeclaration;
use greentic_cap_types::CapabilityId;
use greentic_dw_engine::{EngineDecision, StaticEngine};
use greentic_dw_manifest::{
    DigitalWorkerManifest, LocaleContract, MANIFEST_SCHEMA_VERSION, RequestScope, TeamPolicy,
    TenancyContract,
};
use greentic_dw_runtime::DwRuntime;
use greentic_dw_types::{
    DigitalWorkerTemplate, DwAgentResolveRequest, DwCompositionResolveRequest, DwProviderCatalog,
    DwProviderCatalogEntry, DwResolutionMode, DwWizardQuestionAssembly, DwWizardQuestionBlock,
    LocalePropagation, OutputLocaleGuidance, QuestionDepthMode, QuestionPhase, QuestionScope,
    QuestionSource, QuestionVisibility, TaskEnvelope, TemplateCatalog, TemplateCatalogEntry,
    WorkerLocalePolicy,
};
use schemars::schema_for;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn run_from_env() -> Result<(), CliError> {
    run(std::env::args())
}

pub fn run<I, T>(args: I) -> Result<(), CliError>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::parse_from(args);

    match cli.command {
        Command::Wizard(wizard) => run_wizard(wizard),
    }
}

fn run_wizard(args: WizardArgs) -> Result<(), CliError> {
    if args.schema {
        let schema = schema_for!(AnswerDocument);
        println!("{}", serde_json::to_string_pretty(&schema)?);
        return Ok(());
    }

    if args.list_templates {
        let catalog_path = args.template_catalog.as_ref().ok_or_else(|| {
            CliError::TemplateCatalogPathRequired {
                usage: "--list-templates".to_string(),
            }
        })?;
        let catalog = TemplateCatalog::from_json_path(catalog_path)?;
        let output = WizardOutput {
            contract_version: CONTRACT_VERSION.to_string(),
            command: "wizard".to_string(),
            mode: "template_catalog".to_string(),
            answers: None,
            data: serde_json::json!({
                "catalog_path": catalog_path,
                "entries": catalog.entries,
            }),
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    let mut answers = if let Some(path) = &args.answers {
        load_answers(path)?
    } else {
        AnswerDocument {
            manifest_id: String::new(),
            display_name: String::new(),
            manifest_version: "0.5".to_string(),
            tenant: String::new(),
            template_id: None,
            review_mode: None,
            provider_overrides: BTreeMap::new(),
            design_answers: BTreeMap::new(),
            agent_answers: BTreeMap::new(),
            team: None,
            requested_locale: None,
            human_locale: None,
            worker_default_locale: "en-US".to_string(),
        }
    };

    let mut template_catalog_path = args.template_catalog.clone();
    let mut provider_catalog_path = args.provider_catalog.clone();
    if !args.non_interactive {
        if template_catalog_path.is_none() {
            template_catalog_path = discover_starter_catalog_path("templates/catalog.json");
        }
        if provider_catalog_path.is_none() {
            provider_catalog_path = discover_starter_catalog_path("providers/catalog.json");
        }
    }

    let selected_template = if let Some(template_id) = &args.template {
        let catalog_path = template_catalog_path.as_ref().ok_or_else(|| {
            CliError::TemplateCatalogPathRequired {
                usage: "--template".to_string(),
            }
        })?;
        let catalog = TemplateCatalog::from_json_path(catalog_path)?;
        let selected_entry = catalog.find(template_id).cloned();
        let template = catalog.resolve_template(template_id)?;
        apply_template_defaults(&mut answers, &template);
        Some((template, selected_entry))
    } else if !args.non_interactive {
        if let Some(catalog_path) = template_catalog_path.as_ref() {
            let catalog = TemplateCatalog::from_json_path(catalog_path)?;
            if let Some(template_id) = prompt_template_selection(&catalog, prompt_text)? {
                let selected_entry = catalog.find(&template_id).cloned();
                let template = catalog.resolve_template(&template_id)?;
                apply_template_defaults(&mut answers, &template);
                Some((template, selected_entry))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let provider_catalog = if let Some(path) = &provider_catalog_path {
        Some(DwProviderCatalog::from_json_path(path)?)
    } else {
        None
    };

    apply_overrides(&mut answers, &args)?;

    if !args.non_interactive {
        if let Some((template, selected_entry)) = selected_template.as_ref() {
            prompt_design_flow_with(
                &mut answers,
                template,
                selected_entry.as_ref(),
                provider_catalog.as_ref(),
                prompt_text,
            )?;
        }
        prompt_if_missing(&mut answers, &args.locale)?;
    }

    let manifest = build_manifest(&answers);
    manifest.validate()?;

    let request_scope = RequestScope {
        tenant: answers.tenant.clone(),
        team: answers.team.clone(),
    };

    let mut envelope = manifest.to_task_envelope(
        format!("{}-task", answers.manifest_id),
        answers.manifest_id.clone(),
        &request_scope,
        answers.requested_locale.clone(),
        answers.human_locale.clone(),
    )?;

    let output = if args.dry_run {
        build_dry_run_output(
            &manifest,
            &envelope,
            &answers,
            selected_template.as_ref().map(|(template, _)| template),
            selected_template
                .as_ref()
                .and_then(|(_, selected_entry)| selected_entry.as_ref()),
            provider_catalog.as_ref(),
            args.emit_answers,
        )?
    } else {
        let runtime = DwRuntime::new(StaticEngine::new(EngineDecision::Batch(vec![
            greentic_dw_core::RuntimeOperation::Start,
            greentic_dw_core::RuntimeOperation::Complete,
        ])));

        let events = runtime.tick(&mut envelope)?;

        WizardOutput {
            contract_version: CONTRACT_VERSION.to_string(),
            command: "wizard".to_string(),
            mode: "execute".to_string(),
            answers: args.emit_answers.then_some(answers),
            data: serde_json::json!({
                "final_state": format!("{:?}", envelope.state),
                "event_count": events.len(),
                "events": events.iter().map(|e| e.operation.name()).collect::<Vec<_>>(),
            }),
        }
    };

    let mut output = output;
    if let Some((template, _)) = selected_template
        && let Some(data) = output.data.as_object_mut()
    {
        data.insert(
            "selected_template".to_string(),
            serde_json::json!({
                "id": template.metadata.id,
                "name": template.metadata.name,
                "supports_multi_agent_app_pack": template.supports_multi_agent_app_pack,
            }),
        );
    }

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn discover_starter_catalog_path(relative: &str) -> Option<PathBuf> {
    let candidate = PathBuf::from("examples").join(relative);
    if candidate.exists() {
        Some(candidate)
    } else {
        None
    }
}

pub(crate) fn prompt_template_selection<F>(
    catalog: &TemplateCatalog,
    mut prompt_fn: F,
) -> Result<Option<String>, io::Error>
where
    F: FnMut(&str) -> Result<String, io::Error>,
{
    if catalog.entries.is_empty() {
        return Ok(None);
    }

    let mut prompt = String::from("Select a digital worker template:\n");
    for (index, entry) in catalog.entries.iter().enumerate() {
        prompt.push_str(&format!(
            "{}. {} ({})\n",
            index + 1,
            entry.display_name,
            entry.template_id
        ));
    }
    prompt.push_str("Enter template number [1]: ");

    let response = prompt_fn(&prompt)?;
    let trimmed = response.trim();
    let selected_index = if trimmed.is_empty() {
        0
    } else {
        trimmed.parse::<usize>().unwrap_or(0).saturating_sub(1)
    };

    Ok(catalog
        .entries
        .get(selected_index)
        .or_else(|| catalog.entries.first())
        .map(|entry| entry.template_id.clone()))
}

pub(crate) fn build_dry_run_output(
    manifest: &DigitalWorkerManifest,
    envelope: &TaskEnvelope,
    answers: &AnswerDocument,
    selected_template: Option<&DigitalWorkerTemplate>,
    selected_template_entry: Option<&TemplateCatalogEntry>,
    provider_catalog: Option<&DwProviderCatalog>,
    emit_answers: bool,
) -> Result<WizardOutput, CliError> {
    let effective_locale = envelope
        .locale
        .resolve_effective_locale()
        .map(str::to_string);
    let review_mode = answers.review_mode.unwrap_or(DwResolutionMode::Recommended);

    let mut data = serde_json::json!({
        "manifest_id": manifest.id,
        "tenant": envelope.scope.tenant,
        "team": envelope.scope.team,
        "state": format!("{:?}", envelope.state),
        "effective_locale": effective_locale,
        "requested_locale": envelope.locale.requested_locale,
        "human_locale": envelope.locale.human_locale,
        "review_mode": review_mode,
    });

    if let Some(template) = selected_template {
        let question_assembly =
            build_question_assembly(template, selected_template_entry, provider_catalog)?;
        let review_envelope =
            build_review_envelope(answers, template, selected_template_entry, provider_catalog)?;
        if let Some(obj) = data.as_object_mut() {
            let visible_design_blocks = question_assembly
                .blocks_for(
                    QuestionPhase::Design,
                    if matches!(review_mode, DwResolutionMode::ReviewAll) {
                        QuestionDepthMode::ReviewAll
                    } else {
                        QuestionDepthMode::Recommended
                    },
                    false,
                )
                .into_iter()
                .cloned()
                .collect::<Vec<_>>();
            let visible_setup_blocks = question_assembly
                .blocks
                .iter()
                .filter(|block| block.phase == QuestionPhase::Setup)
                .filter(|block| setup_block_is_required(block, &review_envelope))
                .cloned()
                .collect::<Vec<_>>();
            let resolved_visible_design_blocks =
                resolve_visible_blocks_for_output(&visible_design_blocks, answers, template);
            let resolved_visible_setup_blocks =
                resolve_visible_blocks_for_output(&visible_setup_blocks, answers, template);
            obj.insert(
                "question_assembly".to_string(),
                serde_json::to_value(&question_assembly)?,
            );
            obj.insert(
                "visible_design_blocks".to_string(),
                serde_json::to_value(&visible_design_blocks)?,
            );
            obj.insert(
                "visible_setup_blocks".to_string(),
                serde_json::to_value(&visible_setup_blocks)?,
            );
            obj.insert(
                "resolved_visible_design_blocks".to_string(),
                serde_json::to_value(resolved_visible_design_blocks)?,
            );
            obj.insert(
                "resolved_visible_setup_blocks".to_string(),
                serde_json::to_value(resolved_visible_setup_blocks)?,
            );
            obj.insert(
                "review_envelope".to_string(),
                serde_json::to_value(review_envelope)?,
            );
        }
    }

    Ok(WizardOutput {
        contract_version: CONTRACT_VERSION.to_string(),
        command: "wizard".to_string(),
        mode: "dry_run".to_string(),
        answers: emit_answers.then_some(answers.clone()),
        data,
    })
}

pub(crate) fn build_review_envelope(
    answers: &AnswerDocument,
    template: &DigitalWorkerTemplate,
    selected_template_entry: Option<&TemplateCatalogEntry>,
    provider_catalog: Option<&DwProviderCatalog>,
) -> Result<greentic_dw_types::DwReviewEnvelope, CliError> {
    let provider_catalog = provider_catalog.cloned().unwrap_or_default();
    let agent_count = resolved_agent_count(answers, template);
    let use_shared_provider_overrides = uses_shared_provider_strategy(answers);
    let shared_provider_overrides = if use_shared_provider_overrides {
        collect_selected_provider_overrides(answers, template, &provider_catalog)
    } else {
        BTreeMap::new()
    };
    let request = DwCompositionResolveRequest {
        application_id: answers.manifest_id.clone(),
        display_name: answers.display_name.clone(),
        version: Some(answers.manifest_version.clone()),
        tenant: Some(answers.tenant.clone()),
        tags: Vec::new(),
        agents: (0..agent_count)
            .map(|index| {
                let agent_id = if agent_count == 1 {
                    if answers.manifest_id.trim().is_empty() {
                        "agent-1".to_string()
                    } else {
                        answers.manifest_id.clone()
                    }
                } else if answers.manifest_id.trim().is_empty() {
                    format!("agent-{}", index + 1)
                } else {
                    format!("{}.agent-{}", answers.manifest_id, index + 1)
                };

                DwAgentResolveRequest {
                    agent_id: agent_id.clone(),
                    display_name: Some(if agent_count == 1 {
                        answers.display_name.clone()
                    } else {
                        format!("{} {}", answers.display_name, index + 1)
                    }),
                    template: template.clone(),
                    selected_template: selected_template_entry.cloned(),
                    answers: scoped_agent_design_answers(answers, &agent_id),
                    provider_overrides: scoped_agent_provider_overrides(
                        answers,
                        template,
                        &provider_catalog,
                        &agent_id,
                        use_shared_provider_overrides,
                    ),
                }
            })
            .collect(),
        shared_provider_overrides,
        mode: Some(answers.review_mode.unwrap_or(DwResolutionMode::Recommended)),
    };

    request
        .resolve(&provider_catalog)?
        .to_review_envelope()
        .map_err(Into::into)
}

fn resolved_agent_count(answers: &AnswerDocument, template: &DigitalWorkerTemplate) -> usize {
    if !template.supports_multi_agent_app_pack {
        return 1;
    }

    answers
        .design_answers
        .get("agent_count")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|count| *count > 1)
        .unwrap_or(1)
}

fn uses_shared_provider_strategy(answers: &AnswerDocument) -> bool {
    let Some(value) = answers
        .design_answers
        .get("provider_strategy")
        .and_then(serde_json::Value::as_str)
    else {
        return true;
    };

    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "n" | "no" | "per_agent" | "per-agent" | "local"
    )
}

fn resolve_visible_blocks_for_output(
    blocks: &[DwWizardQuestionBlock],
    answers: &AnswerDocument,
    template: &DigitalWorkerTemplate,
) -> Vec<DwWizardQuestionBlock> {
    let agent_count = resolved_agent_count(answers, template);
    let shared_provider_strategy = uses_shared_provider_strategy(answers);

    if agent_count <= 1 {
        return blocks.to_vec();
    }

    let mut resolved = Vec::new();
    for block in blocks {
        match &block.scope {
            QuestionScope::Agent { agent_id: None } => {
                for index in 0..agent_count {
                    resolved.push(scoped_visible_block_for_output(block, index + 1, true));
                }
            }
            QuestionScope::Provider { agent_id: None, .. } if !shared_provider_strategy => {
                for index in 0..agent_count {
                    resolved.push(scoped_visible_block_for_output(block, index + 1, true));
                }
            }
            _ => resolved.push(block.clone()),
        }
    }

    resolved
}

fn scoped_visible_block_for_output(
    block: &DwWizardQuestionBlock,
    agent_index: usize,
    include_scope_prefix: bool,
) -> DwWizardQuestionBlock {
    let mut scoped = block.clone();
    let agent_id = format!("agent-{agent_index}");
    scoped.block_id = format!("{}.agent-{agent_index}", block.block_id);
    scoped.path = format!("{}.agent-{agent_index}", block.path);
    scoped.prompt = block
        .prompt
        .as_ref()
        .map(|prompt| format!("Agent {agent_index}: {prompt}"));
    scoped.answer_key = block
        .answer_key
        .as_ref()
        .map(|answer_key| scoped_answer_key_for_output(block, answer_key, agent_index));
    scoped.scope = match &block.scope {
        QuestionScope::Agent { .. } => QuestionScope::Agent {
            agent_id: Some(agent_id),
        },
        QuestionScope::Provider { provider_id, .. } => QuestionScope::Provider {
            provider_id: provider_id.clone(),
            agent_id: Some(format!("agent-{agent_index}")),
        },
        _ if include_scope_prefix => QuestionScope::Agent {
            agent_id: Some(agent_id),
        },
        _ => block.scope.clone(),
    };
    scoped
}

fn scoped_answer_key_for_output(
    block: &DwWizardQuestionBlock,
    answer_key: &str,
    agent_index: usize,
) -> String {
    if block.block_id.starts_with("provider.select.") {
        format!("agent.{agent_index}.provider.{answer_key}")
    } else {
        format!("agent.{agent_index}.{answer_key}")
    }
}

fn scoped_agent_design_answers(
    answers: &AnswerDocument,
    agent_id: &str,
) -> BTreeMap<String, serde_json::Value> {
    let mut scoped_answers = answers
        .design_answers
        .iter()
        .filter(|(key, _)| !key.starts_with("agent."))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();

    if let Some(explicit_answers) = find_agent_answers(answers, agent_id) {
        scoped_answers.extend(explicit_answers.design_answers.clone());
    }

    let numeric_prefix = agent_numeric_answer_prefix(agent_id);
    let stable_id_prefix = format!("agent.{agent_id}.");

    for (key, value) in &answers.design_answers {
        if let Some(scoped_key) = key.strip_prefix(&numeric_prefix) {
            scoped_answers.insert(scoped_key.to_string(), value.clone());
        } else if let Some(scoped_key) = key.strip_prefix(&stable_id_prefix) {
            scoped_answers.insert(scoped_key.to_string(), value.clone());
        }
    }

    scoped_answers
}

fn scoped_agent_provider_overrides(
    answers: &AnswerDocument,
    template: &DigitalWorkerTemplate,
    provider_catalog: &DwProviderCatalog,
    agent_id: &str,
    use_shared_provider_overrides: bool,
) -> BTreeMap<CapabilityId, String> {
    let mut scoped_overrides = if use_shared_provider_overrides {
        BTreeMap::new()
    } else {
        collect_selected_provider_overrides(answers, template, provider_catalog)
    };

    if let Some(explicit_answers) = find_agent_answers(answers, agent_id) {
        for (capability_id, provider_id) in &explicit_answers.provider_overrides {
            if let Ok(capability_id) = CapabilityId::new(capability_id.clone()) {
                scoped_overrides.insert(capability_id, provider_id.clone());
            }
        }
    }

    let numeric_prefix = format!("{}provider.", agent_numeric_answer_prefix(agent_id));
    let stable_id_prefix = format!("agent.{agent_id}.provider.");

    for (key, value) in &answers.design_answers {
        let scoped_capability = key
            .strip_prefix(&numeric_prefix)
            .or_else(|| key.strip_prefix(&stable_id_prefix));
        let Some(capability_id) = scoped_capability else {
            continue;
        };
        let Some(provider_id) = value.as_str() else {
            continue;
        };
        let Ok(capability_id) = CapabilityId::new(capability_id.to_string()) else {
            continue;
        };
        scoped_overrides.insert(capability_id, provider_id.to_string());
    }

    scoped_overrides
}

fn find_agent_answers<'a>(
    answers: &'a AnswerDocument,
    agent_id: &str,
) -> Option<&'a AgentAnswerDocument> {
    answers.agent_answers.get(agent_id).or_else(|| {
        answers
            .agent_answers
            .get(&format!("agent-{}", agent_numeric_suffix(agent_id)))
    })
}

fn agent_numeric_answer_prefix(agent_id: &str) -> String {
    let suffix = agent_numeric_suffix(agent_id);
    format!("agent.{suffix}.")
}

fn agent_numeric_suffix(agent_id: &str) -> &str {
    agent_id
        .rsplit_once("agent-")
        .map(|(_, numeric)| numeric)
        .filter(|numeric| !numeric.is_empty())
        .unwrap_or("1")
}

fn upsert_agent_answer_document<'a>(
    answers: &'a mut AnswerDocument,
    agent_id: &str,
) -> &'a mut AgentAnswerDocument {
    answers
        .agent_answers
        .entry(agent_id.to_string())
        .or_default()
}

fn setup_block_is_required(
    block: &DwWizardQuestionBlock,
    review_envelope: &greentic_dw_types::DwReviewEnvelope,
) -> bool {
    review_envelope
        .setup_requirements
        .iter()
        .any(|requirement| {
            requirement.question_block_id.as_deref() == Some(block.block_id.as_str())
                || match (&block.source, requirement.provider_id.as_deref()) {
                    (QuestionSource::Provider { provider_id }, Some(required_provider_id)) => {
                        provider_id == required_provider_id
                    }
                    _ => false,
                }
        })
}

pub(crate) fn build_question_assembly(
    template: &DigitalWorkerTemplate,
    selected_template_entry: Option<&TemplateCatalogEntry>,
    provider_catalog: Option<&DwProviderCatalog>,
) -> Result<DwWizardQuestionAssembly, CliError> {
    let mut blocks = vec![
        DwWizardQuestionBlock {
            block_id: "core.app.identity".to_string(),
            source: QuestionSource::DwCore,
            owner: "core".to_string(),
            path: "core.app.identity".to_string(),
            answer_key: None,
            prompt: None,
            scope: QuestionScope::Application,
            phase: QuestionPhase::Design,
            visibility: QuestionVisibility::Required,
            source_ref: None,
            summary: Some("Application identity and manifest questions".to_string()),
        },
        DwWizardQuestionBlock {
            block_id: "core.agent.topology".to_string(),
            source: QuestionSource::DwCore,
            owner: "core".to_string(),
            path: "core.agent.topology".to_string(),
            answer_key: Some("agent_count".to_string()),
            prompt: Some("Enter agent count [1]: ".to_string()),
            scope: QuestionScope::Application,
            phase: QuestionPhase::Design,
            visibility: if template.supports_multi_agent_app_pack {
                QuestionVisibility::ReviewAll
            } else {
                QuestionVisibility::Optional
            },
            source_ref: None,
            summary: Some("Choose one worker or a multi-agent application".to_string()),
        },
    ];

    let template_source = selected_template_entry.map(|entry| entry.source_ref.clone());
    let default_block_ids = &template
        .behavior_scaffold
        .default_mode_behavior
        .question_block_ids;
    let review_all_block_ids = &template
        .behavior_scaffold
        .personalised_mode_behavior
        .question_block_ids;
    let mut known_template_blocks = BTreeSet::new();

    for block in &template.question_blocks {
        known_template_blocks.insert(block.block_id.clone());
        blocks.push(DwWizardQuestionBlock {
            block_id: block.block_id.clone(),
            source: QuestionSource::Template {
                template_id: template.metadata.id.clone(),
            },
            owner: format!("template.{}", template.metadata.id),
            path: format!("template.{}.{}", template.metadata.id, block.block_id),
            answer_key: block.answer_key.clone(),
            prompt: block.prompt.clone(),
            scope: QuestionScope::Agent { agent_id: None },
            phase: QuestionPhase::Design,
            visibility: template_block_visibility(
                &block.block_id,
                block.required,
                default_block_ids,
                review_all_block_ids,
            ),
            source_ref: block.source.clone().or_else(|| template_source.clone()),
            summary: block.summary.clone(),
        });
    }

    for block_id in default_block_ids.iter().chain(review_all_block_ids.iter()) {
        if known_template_blocks.insert(block_id.clone()) {
            blocks.push(DwWizardQuestionBlock {
                block_id: block_id.clone(),
                source: QuestionSource::Template {
                    template_id: template.metadata.id.clone(),
                },
                owner: format!("template.{}", template.metadata.id),
                path: format!("template.{}.{}", template.metadata.id, block_id),
                answer_key: None,
                prompt: None,
                scope: QuestionScope::Agent { agent_id: None },
                phase: QuestionPhase::Design,
                visibility: template_block_visibility(
                    block_id,
                    false,
                    default_block_ids,
                    review_all_block_ids,
                ),
                source_ref: template_source.clone(),
                summary: Some(format!(
                    "Template behavior block `{}` for `{}`",
                    block_id, template.metadata.name
                )),
            });
        }
    }

    if let Some(provider_catalog) = provider_catalog {
        for (capability_id, optional) in template
            .capability_plan
            .required_capabilities
            .iter()
            .cloned()
            .map(|capability_id| (capability_id, false))
            .chain(
                template
                    .capability_plan
                    .optional_capabilities
                    .iter()
                    .cloned()
                    .map(|capability_id| (capability_id, true)),
            )
        {
            if let Some(provider) =
                select_provider_for_capability(template, provider_catalog, &capability_id)
            {
                blocks.push(DwWizardQuestionBlock {
                    block_id: format!("provider.select.{}", provider.provider_id),
                    source: QuestionSource::Provider {
                        provider_id: provider.provider_id.clone(),
                    },
                    owner: format!("provider.{}", provider.provider_id),
                    path: format!(
                        "provider.{}.selection.{}",
                        provider.provider_id,
                        capability_slug(&capability_id)
                    ),
                    answer_key: Some(capability_id.as_str().to_string()),
                    prompt: Some(format!(
                        "Provider override for {} (Enter to keep {}): ",
                        capability_id.as_str(),
                        provider.provider_id
                    )),
                    scope: QuestionScope::Provider {
                        provider_id: Some(provider.provider_id.clone()),
                        agent_id: None,
                    },
                    phase: QuestionPhase::Design,
                    visibility: if optional {
                        QuestionVisibility::ReviewAll
                    } else {
                        QuestionVisibility::Required
                    },
                    source_ref: None,
                    summary: Some(format!(
                        "Recommended provider `{}` for capability `{}`",
                        provider.display_name,
                        capability_id.as_str()
                    )),
                });

                let mut emitted_provider_blocks = BTreeSet::new();
                let provider_block_ids = provider
                    .required_question_block_ids
                    .iter()
                    .cloned()
                    .chain(
                        provider
                            .question_blocks
                            .iter()
                            .map(|block| block.block_id.clone()),
                    )
                    .filter(|block_id| emitted_provider_blocks.insert(block_id.clone()))
                    .collect::<Vec<_>>();

                for question_block_id in provider_block_ids {
                    let provider_block = provider
                        .question_blocks
                        .iter()
                        .find(|candidate| candidate.block_id == question_block_id);
                    blocks.push(DwWizardQuestionBlock {
                        block_id: question_block_id.clone(),
                        source: QuestionSource::Provider {
                            provider_id: provider.provider_id.clone(),
                        },
                        owner: format!("provider.{}", provider.provider_id),
                        path: format!("provider.{}.{}", provider.provider_id, question_block_id),
                        answer_key: provider_block
                            .and_then(|block| block.answer_key.clone())
                            .or_else(|| Some(question_block_id.clone())),
                        prompt: provider_block
                            .and_then(|block| block.prompt.clone())
                            .or_else(|| {
                                Some(format!(
                                    "Enter value for provider block `{}`: ",
                                    question_block_id
                                ))
                            }),
                        scope: QuestionScope::Provider {
                            provider_id: Some(provider.provider_id.clone()),
                            agent_id: None,
                        },
                        phase: if provider_block
                            .and_then(|block| block.setup_schema_ref.as_ref())
                            .is_some()
                            || !provider.required_setup_schema_refs.is_empty()
                        {
                            QuestionPhase::Setup
                        } else {
                            QuestionPhase::Design
                        },
                        visibility: if provider_block
                            .and_then(|block| block.setup_schema_ref.as_ref())
                            .is_some()
                            || !provider.required_setup_schema_refs.is_empty()
                        {
                            QuestionVisibility::HiddenUnlessNeeded
                        } else {
                            QuestionVisibility::ReviewAll
                        },
                        source_ref: provider_block
                            .and_then(|block| block.setup_schema_ref.clone())
                            .or_else(|| provider.required_setup_schema_refs.first().cloned()),
                        summary: Some(format!(
                            "Follow-up questions for provider `{}`",
                            provider.display_name
                        )),
                    });
                }
            }
        }
    }

    if template.supports_multi_agent_app_pack {
        blocks.push(DwWizardQuestionBlock {
            block_id: "composition.shared.provider_strategy".to_string(),
            source: QuestionSource::Composition,
            owner: "composition.shared".to_string(),
            path: "composition.shared.provider_strategy".to_string(),
            answer_key: Some("provider_strategy".to_string()),
            prompt: Some("Use shared providers across agents? [Y/n]: ".to_string()),
            scope: QuestionScope::SharedComposition,
            phase: QuestionPhase::Design,
            visibility: QuestionVisibility::ReviewAll,
            source_ref: None,
            summary: Some("Shared versus per-agent provider strategy".to_string()),
        });
    }

    if !template.packaging_hints.support_pack_refs.is_empty()
        || !template.packaging_hints.bundle_notes.is_empty()
    {
        blocks.push(DwWizardQuestionBlock {
            block_id: "packaging.bundle_plan".to_string(),
            source: QuestionSource::Packaging,
            owner: "packaging".to_string(),
            path: "packaging.bundle_plan".to_string(),
            answer_key: Some("bundle_plan_notes".to_string()),
            prompt: Some("Enter bundle planning notes (optional): ".to_string()),
            scope: QuestionScope::Application,
            phase: QuestionPhase::Design,
            visibility: QuestionVisibility::ReviewAll,
            source_ref: None,
            summary: Some("Packaging and bundle inclusion hints".to_string()),
        });
    }

    Ok(DwWizardQuestionAssembly { blocks })
}

fn collect_default_provider_overrides(
    template: &DigitalWorkerTemplate,
    provider_catalog: &DwProviderCatalog,
) -> BTreeMap<CapabilityId, String> {
    template
        .capability_plan
        .required_capabilities
        .iter()
        .chain(template.capability_plan.optional_capabilities.iter())
        .filter_map(|capability_id| {
            select_provider_for_capability(template, provider_catalog, capability_id)
                .map(|provider| (capability_id.clone(), provider.provider_id.clone()))
        })
        .collect()
}

fn collect_selected_provider_overrides(
    answers: &AnswerDocument,
    template: &DigitalWorkerTemplate,
    provider_catalog: &DwProviderCatalog,
) -> BTreeMap<CapabilityId, String> {
    let mut overrides = collect_default_provider_overrides(template, provider_catalog);
    for (capability_id, provider_id) in &answers.provider_overrides {
        if let Ok(capability_id) = CapabilityId::new(capability_id.clone()) {
            overrides.insert(capability_id, provider_id.clone());
        }
    }
    overrides
}

fn select_provider_for_capability<'a>(
    template: &DigitalWorkerTemplate,
    provider_catalog: &'a DwProviderCatalog,
    capability_id: &CapabilityId,
) -> Option<&'a DwProviderCatalogEntry> {
    if let Some(provider_id) = template
        .capability_plan
        .default_provider_ids
        .get(capability_id)
        && let Some(provider) = provider_catalog.find(provider_id)
    {
        return Some(provider);
    }

    provider_catalog
        .entries
        .iter()
        .filter(|entry| {
            entry
                .capability_profile
                .capability_contract_ids
                .contains(capability_id)
                && (entry.template_compatibility.is_empty()
                    || entry
                        .template_compatibility
                        .iter()
                        .any(|candidate| candidate == &template.metadata.id))
        })
        .max_by_key(|entry| {
            (
                entry
                    .default_profile
                    .recommended_for_templates
                    .iter()
                    .any(|candidate| candidate == &template.metadata.id),
                entry.default_profile.is_default_choice,
                entry.default_profile.is_recommended_choice,
            )
        })
}

fn template_block_visibility(
    block_id: &str,
    required: bool,
    default_block_ids: &[String],
    review_all_block_ids: &[String],
) -> QuestionVisibility {
    if required
        || default_block_ids
            .iter()
            .any(|candidate| candidate == block_id)
    {
        QuestionVisibility::Required
    } else if review_all_block_ids
        .iter()
        .any(|candidate| candidate == block_id)
    {
        QuestionVisibility::ReviewAll
    } else {
        QuestionVisibility::Optional
    }
}

fn capability_slug(capability_id: &CapabilityId) -> String {
    capability_id
        .as_str()
        .replace("cap://", "")
        .replace('/', ".")
}

pub(crate) fn load_answers(path: &Path) -> Result<AnswerDocument, CliError> {
    let path_display = path.display().to_string();
    let raw = if is_remote_answers_url(&path_display) {
        fetch_answers_from_url(&path_display)?
    } else if is_insecure_http_url(&path_display) {
        return Err(CliError::InsecureAnswersUrl { url: path_display });
    } else {
        fs::read_to_string(path).map_err(|source| CliError::AnswersRead {
            path: path_display.clone(),
            source,
        })?
    };

    serde_json::from_str(&raw).map_err(|source| CliError::AnswersParse {
        path: path_display,
        source,
    })
}

pub(crate) fn is_remote_answers_url(value: &str) -> bool {
    value.starts_with("https://")
}

pub(crate) fn is_insecure_http_url(value: &str) -> bool {
    value.starts_with("http://")
}

fn fetch_answers_from_url(url: &str) -> Result<String, CliError> {
    let mut response = ureq::get(url)
        .call()
        .map_err(|error| CliError::AnswersFetch {
            url: url.to_string(),
            message: error.to_string(),
        })?;

    response
        .body_mut()
        .read_to_string()
        .map_err(|error| CliError::AnswersFetch {
            url: url.to_string(),
            message: error.to_string(),
        })
}

pub(crate) fn apply_overrides(
    answers: &mut AnswerDocument,
    args: &WizardArgs,
) -> Result<(), CliError> {
    if let Some(v) = &args.manifest_id {
        answers.manifest_id = v.clone();
    }
    if let Some(v) = &args.display_name {
        answers.display_name = v.clone();
    }
    if let Some(v) = &args.tenant {
        answers.tenant = v.clone();
    }
    if let Some(v) = &args.team {
        answers.team = Some(v.clone());
    }
    if let Some(v) = &args.requested_locale {
        answers.requested_locale = Some(v.clone());
    }
    if let Some(v) = &args.human_locale {
        answers.human_locale = Some(v.clone());
    }
    if args.review_all {
        answers.review_mode = Some(DwResolutionMode::ReviewAll);
    }
    for value in &args.provider_overrides {
        let (capability_id, provider_id) = parse_provider_override(value)?;
        answers
            .provider_overrides
            .insert(capability_id, provider_id);
    }
    Ok(())
}

pub(crate) fn apply_template_defaults(
    answers: &mut AnswerDocument,
    template: &DigitalWorkerTemplate,
) {
    if answers.manifest_id.trim().is_empty() {
        answers.manifest_id = template.metadata.id.clone();
    }
    if answers.template_id.is_none() {
        answers.template_id = Some(template.metadata.id.clone());
    }
    if answers.display_name.trim().is_empty() {
        answers.display_name = template.metadata.name.clone();
    }

    for (key, value) in &template.defaults.values {
        match (key.as_str(), value.as_str()) {
            ("manifest_id", Some(v)) if answers.manifest_id.trim().is_empty() => {
                answers.manifest_id = v.to_string()
            }
            ("display_name", Some(v)) if answers.display_name.trim().is_empty() => {
                answers.display_name = v.to_string()
            }
            ("manifest_version", Some(v)) if answers.manifest_version.trim().is_empty() => {
                answers.manifest_version = v.to_string()
            }
            ("tenant", Some(v)) if answers.tenant.trim().is_empty() => {
                answers.tenant = v.to_string()
            }
            ("agent_count", Some(v)) if !answers.design_answers.contains_key("agent_count") => {
                answers.design_answers.insert(
                    "agent_count".to_string(),
                    serde_json::Value::String(v.to_string()),
                );
            }
            ("team", Some(v)) if answers.team.is_none() => answers.team = Some(v.to_string()),
            ("requested_locale", Some(v)) if answers.requested_locale.is_none() => {
                answers.requested_locale = Some(v.to_string())
            }
            ("human_locale", Some(v)) if answers.human_locale.is_none() => {
                answers.human_locale = Some(v.to_string())
            }
            ("worker_default_locale", Some(v))
                if answers.worker_default_locale.trim().is_empty() =>
            {
                answers.worker_default_locale = v.to_string()
            }
            _ => {}
        }
    }
}

fn prompt_if_missing(answers: &mut AnswerDocument, locale: &str) -> Result<(), io::Error> {
    if answers.manifest_id.trim().is_empty() {
        answers.manifest_id = prompt(locale, MsgKey::ManifestId)?;
    }
    if answers.display_name.trim().is_empty() {
        answers.display_name = prompt(locale, MsgKey::DisplayName)?;
    }
    if answers.tenant.trim().is_empty() {
        answers.tenant = prompt(locale, MsgKey::Tenant)?;
    }
    if answers.team.is_none() {
        let v = prompt(locale, MsgKey::Team)?;
        if !v.trim().is_empty() {
            answers.team = Some(v);
        }
    }
    if answers.requested_locale.is_none() {
        let v = prompt(locale, MsgKey::RequestedLocale)?;
        if !v.trim().is_empty() {
            answers.requested_locale = Some(v);
        }
    }
    if answers.human_locale.is_none() {
        let v = prompt(locale, MsgKey::HumanLocale)?;
        if !v.trim().is_empty() {
            answers.human_locale = Some(v);
        }
    }

    Ok(())
}

pub(crate) fn prompt_design_flow_with<F>(
    answers: &mut AnswerDocument,
    template: &DigitalWorkerTemplate,
    selected_template_entry: Option<&TemplateCatalogEntry>,
    provider_catalog: Option<&DwProviderCatalog>,
    mut prompt_fn: F,
) -> Result<(), io::Error>
where
    F: FnMut(&str) -> Result<String, io::Error>,
{
    if answers.review_mode.is_none() {
        let mode =
            prompt_fn("Use the recommended setup and only answer required questions? [Y/n]: ")?;
        let normalized = mode.trim().to_ascii_lowercase();
        answers.review_mode = Some(if normalized == "n" || normalized == "no" {
            DwResolutionMode::ReviewAll
        } else {
            DwResolutionMode::Recommended
        });
    }

    let assembly = build_question_assembly(template, selected_template_entry, provider_catalog)
        .map_err(io::Error::other)?;
    let depth = if matches!(answers.review_mode, Some(DwResolutionMode::ReviewAll)) {
        QuestionDepthMode::ReviewAll
    } else {
        QuestionDepthMode::Recommended
    };
    let visible_design_blocks = assembly
        .blocks_for(QuestionPhase::Design, depth, false)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();

    for block in &visible_design_blocks {
        if block.path == "core.agent.topology"
            && !answers.design_answers.contains_key("agent_count")
            && template.supports_multi_agent_app_pack
        {
            let response = prompt_fn(block.prompt.as_deref().unwrap_or("Enter agent count [1]: "))?;
            let trimmed = response.trim();
            if !trimmed.is_empty() && trimmed != "1" {
                answers.design_answers.insert(
                    "agent_count".to_string(),
                    serde_json::Value::String(trimmed.to_string()),
                );
            }
        } else if block.answer_key.as_deref() == Some("provider_strategy")
            && !answers.design_answers.contains_key("provider_strategy")
        {
            let prompt = block
                .prompt
                .clone()
                .unwrap_or_else(|| "Use shared providers across agents? [Y/n]: ".to_string());
            let response = prompt_fn(&prompt)?;
            let trimmed = response.trim();
            if !trimmed.is_empty() {
                answers.design_answers.insert(
                    "provider_strategy".to_string(),
                    serde_json::Value::String(trimmed.to_string()),
                );
            }
        }
    }

    let resolved_visible_design_blocks =
        resolve_visible_blocks_for_output(&visible_design_blocks, answers, template);

    for block in &resolved_visible_design_blocks {
        if block.path == "core.agent.topology"
            || block.answer_key.as_deref() == Some("provider_strategy")
        {
            continue;
        } else if block.block_id.starts_with("provider.select.") {
            let Some(recommended_provider_id) = provider_id_from_block(block) else {
                continue;
            };
            let capability_id = block
                .answer_key
                .clone()
                .or_else(|| capability_id_from_selection_path(&block.path));
            let Some(capability_id) = capability_id else {
                continue;
            };
            let prompt = block.prompt.clone().unwrap_or_else(|| {
                format!(
                    "Provider override for {} (Enter to keep {}): ",
                    capability_id, recommended_provider_id
                )
            });
            let scope_agent_id = match &block.scope {
                QuestionScope::Provider { agent_id, .. } => agent_id.as_deref(),
                _ => None,
            };
            if scope_agent_id.is_none() && answers.provider_overrides.contains_key(&capability_id) {
                continue;
            }
            if scope_agent_id.is_some() && answers.design_answers.contains_key(&capability_id) {
                continue;
            }
            let response = prompt_fn(&prompt)?;
            let trimmed = response.trim();
            if !trimmed.is_empty() {
                if scope_agent_id.is_some() {
                    if let Some(agent_id) = scope_agent_id {
                        let scoped_answer_key = capability_id.clone();
                        let capability_id = scoped_answer_key
                            .trim_start_matches(&format!(
                                "{}provider.",
                                agent_numeric_answer_prefix(agent_id)
                            ))
                            .trim_start_matches(&format!("agent.{agent_id}.provider."))
                            .to_string();
                        upsert_agent_answer_document(answers, agent_id)
                            .provider_overrides
                            .insert(capability_id, trimmed.to_string());
                    }
                    answers.design_answers.insert(
                        capability_id,
                        serde_json::Value::String(trimmed.to_string()),
                    );
                } else {
                    answers
                        .provider_overrides
                        .insert(capability_id, trimmed.to_string());
                }
            }
        } else if let Some(answer_key) = &block.answer_key
            && !answers.design_answers.contains_key(answer_key)
        {
            let prompt = block
                .prompt
                .clone()
                .unwrap_or_else(|| format!("Enter value for {}: ", answer_key));
            let response = prompt_fn(&prompt)?;
            let trimmed = response.trim();
            if !trimmed.is_empty() {
                if let Some(agent_id) = match &block.scope {
                    QuestionScope::Agent {
                        agent_id: Some(agent_id),
                    }
                    | QuestionScope::Provider {
                        agent_id: Some(agent_id),
                        ..
                    } => Some(agent_id.as_str()),
                    _ => None,
                } {
                    let scoped_key = answer_key
                        .trim_start_matches(&agent_numeric_answer_prefix(agent_id))
                        .trim_start_matches(&format!("agent.{agent_id}."))
                        .to_string();
                    upsert_agent_answer_document(answers, agent_id)
                        .design_answers
                        .insert(scoped_key, serde_json::Value::String(trimmed.to_string()));
                }
                answers.design_answers.insert(
                    answer_key.clone(),
                    serde_json::Value::String(trimmed.to_string()),
                );
            }
        }
    }

    Ok(())
}

pub(crate) fn build_manifest(answers: &AnswerDocument) -> DigitalWorkerManifest {
    DigitalWorkerManifest {
        version: MANIFEST_SCHEMA_VERSION.to_string(),
        id: answers.manifest_id.clone(),
        display_name: answers.display_name.clone(),
        worker_version: Some(answers.manifest_version.clone()),
        capabilities: CapabilityDeclaration::new(),
        tenancy: TenancyContract {
            tenant: answers.tenant.clone(),
            team_policy: TeamPolicy::Optional {
                default_team: answers.team.clone(),
                allow_request_override: true,
            },
        },
        locale: LocaleContract {
            worker_default_locale: answers.worker_default_locale.clone(),
            policy: WorkerLocalePolicy::PreferRequested,
            propagation: LocalePropagation::PropagateToDelegates,
            output: OutputLocaleGuidance::MatchRequested,
        },
        deep_agent: None,
    }
}

fn parse_provider_override(value: &str) -> Result<(String, String), CliError> {
    let Some((capability_id, provider_id)) = value.split_once('=') else {
        return Err(CliError::InvalidProviderOverride {
            value: value.to_string(),
        });
    };
    if capability_id.trim().is_empty() || provider_id.trim().is_empty() {
        return Err(CliError::InvalidProviderOverride {
            value: value.to_string(),
        });
    }
    Ok((
        capability_id.trim().to_string(),
        provider_id.trim().to_string(),
    ))
}

fn prompt_text(message: &str) -> Result<String, io::Error> {
    use std::io::Write;

    let mut stderr = io::stderr().lock();
    write!(stderr, "{message}")?;
    stderr.flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn provider_id_from_block(block: &DwWizardQuestionBlock) -> Option<&str> {
    match &block.source {
        QuestionSource::Provider { provider_id } => Some(provider_id.as_str()),
        _ => None,
    }
}

fn capability_id_from_selection_path(path: &str) -> Option<String> {
    let slug = path.split(".selection.").nth(1)?;
    Some(format!("cap://{}", slug.replace('.', "/")))
}

pub fn print_error(error: &impl Display) {
    eprintln!("greentic-dw: {error}");
}
