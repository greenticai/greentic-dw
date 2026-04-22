# Delegation Sequence

## Overview

Delegation is modeled as an explicit document exchange rather than implicit background behavior.

## Sequence

```text
planner/runtime         delegator            subagent            reflector
      |                    |                    |                   |
      | delegate step      |                    |                   |
      |------------------->|                    |                   |
      | choose_delegate    |                    |                   |
      |------------------->|                    |                   |
      |<-------------------| decision           |                   |
      | build envelope     |                    |                   |
      |------------------->| start_subtask      |                   |
      |                    |------------------->| execute           |
      |                    |<-------------------| result envelope   |
      | merge_result       |                    |                   |
      |------------------->|                    |                   |
      |<-------------------| merge summary      |                   |
      | review merged output|                   |------------------>|
      |<------------------------------------------------------------|
```

## Delegation notes

- `SubtaskEnvelope` is the contract boundary.
- The parent run keeps authority over merge and final review.
- The subagent returns artifacts and status rather than mutating parent state directly.
