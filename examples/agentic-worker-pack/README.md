# Agentic Worker Pack Reuse Example

This example shows how an Agentic Worker (AW) can reuse the existing Digital
Worker composition, application-pack, provider-pack, and bundle-plan path.

It intentionally does not introduce a Telco-X-specific runtime. Telco-X-style
agents and tools are represented as domain templates and provider packs on top
of the existing DW/AW pack model.

## Runtime Model

The public integration calls one coordinator Agentic Worker component. The
coordinator owns the task lifecycle, can call specialist Agentic Workers through
its delegation/tool interface, merges their outputs, and returns one final reply.

The specialist workers are not independent top-level entrypoints in this model.
They are callable AW dependencies with domain-specific tool capabilities.

## What It Contains

- `fixtures/composition.json`
  A three-agent Telco investigation composition: coordinator, traffic
  specialist, and BGP specialist.
- Coordinator behavior blocks for public entrypoint, intent handling,
  delegation policy, and finalization.
- Specialist behavior blocks marking traffic/BGP workers as callable worker
  tools.
- Shared provider dependencies for LLM, delegation, domain tools, observer
  audit, and common support prompts/policies.

## How To Run

```bash
cargo test -p greentic-dw-testing agentic_worker_pack_example_reuses_dw_pack_path
```
