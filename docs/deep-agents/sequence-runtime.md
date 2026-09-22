# Deep Loop Runtime Sequence

## Overview

The deep loop sits on top of the current runtime rather than replacing it.

## Sequence

```text
planner        context        runtime        workspace      reflector      delegator
   |              |              |               |              |              |
   | next_actions |              |               |              |              |
   |------------->|              |               |              |              |
   |<-------------|              |               |              |              |
   |              | build_context|               |              |              |
   |              |------------->|               |              |              |
   |              |<-------------|               |              |              |
   |              |              | tick/step     |              |              |
   |              |              |-------------->|              |              |
   |              |              |<--------------|              |              |
   |              |              | create_artifact              |              |
   |              |              |--------------->|             |              |
   |              |              |<---------------|             |              |
   |              |              | review_step                  |              |
   |              |              |----------------------------->|              |
   |              |              |<-----------------------------|              |
   | revise_plan? |              |               |              |              |
   |<-------------|              |               |              |              |
```

## Runtime notes

- The runtime still applies legal state transitions.
- Engine decisions still flow through the existing `DwRuntime`.
- Reflection can cause revision, continuation, delegation, or failure.
- Completion is checked before the final `complete`.
- An optional `StepExecutor` (`DeepLoopCoordinator::executor`) carries out each
  non-delegate step after the runtime tick; its JSON body replaces the placeholder
  artifact body and is handed to later steps as `prior_outputs`. The run gets a
  tool-call budget of `max_iterations × TOOL_CALLS_PER_ITERATION` (3), at most
  `PER_STEP_TOOL_CAP` (6) per step, reported as `DeepLoopRun::tool_calls_used`.
  With no executor the loop is unchanged. See `greentic-dw-operala-invoker` for the
  LLM tool-calling executor.
