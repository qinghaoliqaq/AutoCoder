# QA Mode Architecture and Execution Plan

## Goal

Add a new high-level `qa` mode to AI Dev Hub as an acceptance gate above subtask implementation.

`qa` is for feature-level or milestone-level readiness checks:
- validate completed subtasks together
- check integration and regressions
- consume existing evidence from `test`, blackboard state, bugs, and logs
- produce a clear go / no-go verdict
- route failures back to `debug` or `code`

`qa` must fit the current architecture without weakening the existing subtask review workflow.

## Positioning

`qa` is not a replacement for `review`.

Responsibilities:
- `review`: subtask-level correctness, code quality, and safety
- `test`: execution layer that runs concrete verification phases and produces evidence
- `qa`: acceptance layer that judges whether a feature or milestone is ready based on project artifacts and test evidence
- `debug`: root-cause investigation when QA identifies a failure but the fix path is unclear
- `code`: implementation work when QA identifies missing or incomplete functionality

This boundary is important:
- `qa` should not become a second copy of `test`
- `qa` should not directly replace `review`
- `qa` should not silently fix issues by itself

## Core Design Decision

For v1, `qa` is a verdict and routing layer, not a heavy execution layer.

That means:
- `qa` may read artifacts, blackboards, logs, and prior reports
- `qa` may inspect whether tests were run and what they proved
- `qa` should not own a large new test runner
- if fresh execution evidence is needed, Director should call `test` before `qa`

This keeps the architecture coherent with the current system, where `test` already exists as the execution-oriented verification skill.

## Workflow

### Current flow

```text
plan
  -> code(subtask A)
  -> review(subtask A)
  -> code(subtask B)
  -> review(subtask B)
  -> ...
  -> review(final security / cleanup)
  -> test
  -> done
```

### Proposed flow with QA

```text
plan
  -> code(subtask A)
  -> review(subtask A)
  -> code(subtask B)
  -> review(subtask B)
  -> ...
  -> review(final security / cleanup)
  -> test
  -> qa(feature or milestone acceptance)
       -> read PLAN / blackboards / bugs / logs / test outputs
       -> judge readiness
       -> PASS | PASS_WITH_CONCERNS | FAIL
       -> if FAIL and root cause unclear: debug
       -> if FAIL and implementation missing: code
       -> after fix: test -> qa
  -> ship / done
```

## Routing Rules

Add `qa` as a new top-level skill target.

Typical trigger phrases:
- 验收这个功能
- 做端到端验收
- 看看现在能不能上线
- 做里程碑验收
- 回归检查这批改动
- validate this feature
- run QA
- acceptance check
- readiness check
- regression pass

Routing guidance:
- if the request is about one subtask's implementation quality -> `review`
- if the request is about test execution or collecting runtime evidence -> `test`
- if the request is about overall readiness / acceptance / integration -> `qa`
- if QA finds a broken behavior but the fix path is unclear -> `debug`
- if QA finds missing implementation or incomplete delivery -> `code`

## Director-Orchestrated Sequence

For v1, Director should orchestrate the sequence explicitly:

```text
1. finish code + subtask review
2. run final review
3. run test
4. run qa
5. inspect QA verdict
6. if FAIL:
   - route to debug or code
   - then re-run test
   - then re-run qa
7. if PASS or PASS_WITH_CONCERNS:
   - summarize readiness
   - stop or prepare ship handoff
```

Important:
- `qa` does not call `test` internally in v1
- Director remains responsible for sequencing `test -> qa`

## Inputs

Minimum inputs:
- `task`: feature or milestone being validated
- `workspace`: active project path
- `context`: planning and execution context
- optional `issue`: known failure from a previous QA pass

Primary evidence sources:
- `PLAN.md`
- `PLAN_BLACKBOARD.md`
- `PLAN_BLACKBOARD.json`
- `BLACKBOARD.md`
- `BLACKBOARD.json`
- `bugs.md`
- completion reports emitted by `test`
- tool logs
- saved session context

Secondary evidence sources:
- `change.log`

Note on `change.log`:
- treat it as best-effort supporting evidence only
- do not make QA correctness depend on it

## Outputs

`qa` must produce a structured result, not just prose.

Required verdict values:
- `PASS`
- `PASS_WITH_CONCERNS`
- `FAIL`

Required report sections:
- verdict
- scope checked
- evidence used
- issues by severity
- recommended next step

Required next-step values:
- `complete`
- `review`
- `debug`
- `code`

Suggested textual contract:

```text
QA Verdict: PASS | PASS_WITH_CONCERNS | FAIL

Validated Scope:
- ...

Evidence:
- ...

Issues:
- High: ...
- Medium: ...
- Low: ...

Recommended Next Step:
- complete | review | debug | code
```

## Runtime Result Contract

Natural-language output alone is not enough for reliable orchestration.

For implementation, QA should return a structured result payload analogous to the existing review/test phase result:

```ts
{
  verdict: "PASS" | "PASS_WITH_CONCERNS" | "FAIL",
  recommended_next_step: "complete" | "review" | "debug" | "code",
  summary: string,
  issue: string,
}
```

Why this is required:
- Director loop currently has dedicated handling for `review` and `test`
- without a structured QA result, `qa` would fall into the generic branch and be routed incorrectly

## Responsibilities and Boundaries

QA should:
- validate feature-level behavior across multiple subtasks
- judge whether existing evidence is sufficient
- identify integration gaps and regressions
- summarize readiness clearly
- provide the correct next-step recommendation
- support repeated acceptance passes after fixes

QA should not:
- replace subtask `review`
- become another full `test` runner
- mutate code directly in v1
- bypass `debug` when the failure source is unclear
- invent scope that is not in the plan or produced work

## Minimal Implementation Plan

### 1. Skill registration

Files to update:
- `src-tauri/src/skills/mod.rs`
- `src-tauri/src/director.rs`
- Director prompt definitions

Work:
- register `qa` in skill dispatch
- allow Director to emit `<invoke skill="qa" task="..." />`
- update Director routing instructions so it uses `test -> qa` instead of letting `qa` replace `test`

### 2. New skill module

Create:
- `src-tauri/src/skills/qa.rs`

Responsibilities for v1:
- read acceptance artifacts
- inspect planning and execution state
- read prior test evidence
- generate readiness verdict
- recommend next action

Non-goals for v1:
- no browser automation inside QA
- no separate repair loop inside QA
- no large standalone execution framework

### 3. New prompt

Create:
- `src-tauri/prompts/qa_claude.md`

Prompt requirements:
- reason at feature / milestone level, not at single-subtask level
- focus on acceptance and readiness
- treat `test` outputs as evidence, not as optional decoration
- emit strict verdict plus actionable issues
- recommend exactly one next step

### 4. Prompt loader

Files to update:
- `src-tauri/src/prompts.rs`

Work:
- add `qa_claude` to `Prompts`
- include fallback loading for `qa_claude.md`

### 5. Frontend integration

Files to update:
- `src/types.ts`
- `src/invoke.ts`
- `src/App.tsx`
- `src/components/InputBar.tsx`
- `src/components/ModeActivated.tsx`
- relevant tests

Work:
- add `qa` to `AppMode`
- allow invoke parsing for `qa`
- add mode metadata and placeholder text
- add explicit `qa` handling in the Director loop
- avoid falling back to the generic "run review next" branch after QA

### 6. Result plumbing

Files to update:
- `src-tauri/src/skills/mod.rs`
- `src/App.tsx`

Work:
- add a QA result event or direct return contract
- parse QA verdict in the frontend loop
- branch correctly:
  - `PASS` -> summarize and finish
  - `PASS_WITH_CONCERNS` -> summarize and finish with caveats, or optionally route to `review`
  - `FAIL` -> route to `debug` or `code`

## Recommended v1 State Machine

```text
code/review complete
  -> review(final)
  -> test
  -> qa
      -> PASS -> done
      -> PASS_WITH_CONCERNS -> done with caveats
      -> FAIL + next_step=debug -> debug -> test -> qa
      -> FAIL + next_step=code  -> code  -> review -> test -> qa
```

This is the cleanest fit for the current AI Dev Hub architecture.

## Why This Fits the Existing Codebase

AI Dev Hub already has:
- Director-driven skill routing
- distinct top-level modes
- blackboard artifacts
- test and review result concepts
- session persistence and tool logs

What it does not have yet:
- a feature-level acceptance verdict layer
- a structured post-test readiness gate

`qa` should fill exactly that gap.

## Scope for v1

Include:
- top-level `qa` skill
- structured acceptance verdict
- artifact-aware reasoning
- Director routing updates
- frontend support for the new mode

Exclude:
- browser automation inside `qa`
- screenshot diffing
- deployment verification
- autonomous fix loops inside `qa`
- complex new QA dashboard

## Recommended Implementation Priority

1. Add `qa` to mode types, invoke parsing, and dispatch
2. Add `qa_claude.md` and `qa.rs`
3. Add a structured QA result contract
4. Update Director routing to `test -> qa`
5. Add frontend loop handling for QA verdicts
6. Add tests for the new routing and result parsing

## Final Recommendation

Implement `qa` as an acceptance gate above `test`, not as a replacement for `test`.

The architectural rule for v1 should be:

```text
test produces evidence
qa produces verdict
director decides repair routing
```

If this rule is preserved, `qa` will fit cleanly into AI Dev Hub without duplicating existing responsibilities.
