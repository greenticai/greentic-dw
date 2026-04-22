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
