# greentic-dw-operala-invoker

Production OperalaDispatchInvoker wiring the deep-worker providers into the DeepLoopCoordinator.

This crate is part of the `greentic-dw` workspace.

## Deep-worker settings

The designer sends its `deep_worker` settings in the dispatch input:

- `iterationBudget` — planner iterations (one `next_actions` call each). Reaching it
  ends the run as `budget_exhausted`, not as an error.
- `reflection` — review each step with the LLM; off means every review accepts.
- `delegation` — allow `delegate` steps; off tells the planner not to plan them and
  rewrites any it plans anyway. It does not hide tools.
- `planningModel` — accepted and ignored (one LLM per node).

## Tools

A host can give the deep worker tools:

```rust
let invoker = DeepWorkerInvoker::new(llm).with_tools(Some(tools)); // tools: Arc<dyn DeepWorkerTools>
```

`DeepWorkerTools::list` advertises `ToolSpec { name, description, parameters }`;
`call` runs one. A tool-level failure the model should see is returned as
`Ok(json!({"error": …}))`; an `Err` is an infrastructure failure, recorded in the
step output as an error without failing the run.

Tools are wired only when the list is non-empty AND the LLM reports
`capabilities().tools`. Then:

- the plan request carries an `Available tools: <name — description>; …` constraint;
- every non-delegate step runs a bounded tool-calling loop (`LlmToolStepExecutor`),
  and its JSON output (text answer plus each tool call and result) becomes the step
  artifact, passed to later steps as prior outputs;
- the reply prompt quotes every step's output, so the answer uses the tool results;
- the output gains `tool_calls_used`.

Budget: tool calls are separate from `iterationBudget`. A run may make
`iterationBudget × 3` tool calls in total, and one step at most 6. When the total
is spent, later steps are asked to answer from what they have. Each tool result is
cut to about 8 KB for the model. Results are cached per run by tool name and
canonical arguments, so a step the planner revisits does not repeat a side effect;
a cached hit costs no budget.

Without tools (or with a model that cannot call them) the invoker behaves exactly
as before: same prompts, same output.
