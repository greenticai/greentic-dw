# PR-07: Add digital worker template descriptor model

## Title
`feat(templates): add digital worker template descriptor model`

## Objective
Introduce a declarative template format so DW types are loaded from descriptors rather than hard-coded in the wizard.

## Deliverables
- `DigitalWorkerTemplate`
- `TemplateMetadata`
- `TemplateCapabilityPlan`
- `TemplateQuestionBlockRef`
- `TemplateDefaults`
- `TemplatePackagingHints`
- `TemplateBehaviorScaffold`

## Template Coverage
- template id
- name
- summary
- category and tags
- maturity
- required capabilities
- optional capabilities
- default providers per capability
- default values
- question block references
- behavior scaffold
- packaging hints
- multi-agent support hints
- `supports_multi_agent_app_pack: bool`
- `default_mode_behavior`
- `personalised_mode_behavior`

## Acceptance Criteria
- templates can be loaded from declarative files or descriptor objects
- templates are schema-exportable
- templates can express required vs optional capabilities
- templates can express provider defaults
- templates can express packaging hints for one or many workers
