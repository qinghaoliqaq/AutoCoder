# Vendored Skills Architecture

## Goal

Integrate selected external skills, such as MiniMax skills, into this project **without**:

- writing into the user's global `~/.agents/skills`
- writing into Claude's plugin marketplace area
- depending on Claude/Codex native skill auto-discovery

The application should own the skill assets and decide when they are injected into agent prompts.

## Core Principle

External skills should be treated as:

- vendored prompt assets
- selected by our orchestrator
- injected into Claude/Codex prompts at runtime
- always subordinate to our own architecture

They must **not** replace:

- Director
- shared blackboard workflow
- `plan / code / review / test`
- subtask-level Claude -> Codex -> Claude repair loop

## Why This Architecture

The current system already has a real orchestration layer:

- Director decides mode transitions
- `plan` uses `PLAN_BLACKBOARD.*`
- `code` uses `BLACKBOARD.*`
- work is split into checklist-based subtasks
- Codex inline-reviews each subtask

MiniMax-style skills are useful, but they solve a different layer:

- how one agent should approach a certain kind of task

Our system solves:

- how multiple agents coordinate and converge

So the correct relationship is:

- our orchestrator owns control
- vendored skills are optional execution aids

## Recommended Directory Layout

Add a directory inside this repo, for example:

```text
ai-dev-hub/
  vendor/
    minimax-skills/
      README.md
      skills/
        frontend-dev/
          SKILL.md
        fullstack-dev/
          SKILL.md
```

Alternative naming:

```text
third_party/minimax-skills/
```

Preferred rule:

- keep third-party content isolated from our first-party prompts
- do not mix vendored skills into `src-tauri/prompts/`

## Scope Control

Do **not** vendor every skill by default.

Start with a small allowlist:

- `frontend-dev`
- `fullstack-dev`

Potential later additions:

- backend/API skill if high quality
- testing skill if it aligns with our test mode

Avoid bulk-enabling everything until prompt conflicts are understood.

## Runtime Model

At runtime:

1. Director selects a mode as usual.
2. Our orchestrator identifies the subtask type.
3. Our orchestrator selects zero or one vendored skill.
4. The vendored skill content is read from our repo.
5. The skill content is prepended or merged into the agent prompt.
6. The prompt explicitly states that our blackboard and plan files override any vendored workflow.

This means Claude/Codex never need native skill discovery for these vendored assets.

## Injection Strategy

### Best Place To Inject

Inject vendored skill instructions inside the backend prompt assembly layer, not in the frontend.

Likely insertion points:

- `src-tauri/src/skills/code.rs`
- `src-tauri/src/skills/plan.rs`
- possibly `src-tauri/src/skills/debug.rs`

Do not inject in the browser UI.

### Prompt Precedence

Prompt order should be:

1. Our system-level task contract
2. Shared blackboard contract
3. `PLAN.md` / `PLAN_BLACKBOARD.md` / `BLACKBOARD.md` context
4. Vendored skill guidance
5. Current subtask/task instruction

Reason:

- vendored skill guidance should help execution
- but it must not override coordination protocol

## Required Hard Rule

Every injected prompt using vendored skills should include a guardrail like:

```text
If the vendored skill conflicts with PLAN.md, PLAN_BLACKBOARD.md, BLACKBOARD.md,
or the current subtask contract, follow the local project rules and treat the
vendored skill only as implementation guidance.
```

Without this, external skills may try to re-run their own workflow and fight our orchestrator.

## Suggested Data Model

Add a small internal skill registry.

Example conceptual structure:

```rust
struct VendoredSkill {
    id: String,
    title: String,
    path: PathBuf,
    applies_to: Vec<SkillMode>,
    tags: Vec<String>,
}
```

Possible modes:

- `plan`
- `code`
- `debug`
- `review`
- `test`

Possible tags:

- `frontend`
- `backend`
- `fullstack`
- `ui`
- `api`

## Selection Strategy

Do not ask the model to choose vendored skills blindly.

The orchestrator should choose.

### Recommended Initial Heuristic

For `code` mode:

- if subtask ID starts with `P` -> prefer `frontend-dev`
- if subtask description mentions both UI and API integration -> prefer `fullstack-dev`
- if subtask ID starts with `F` and looks backend-only -> no vendored skill at first

For `plan` mode:

- default to no vendored skill initially
- optionally use `fullstack-dev` only during final synthesis support, not during debate rounds

For `debug` mode:

- only add vendored skill later if a high-confidence debugging skill exists

## Recommended Implementation Phases

### Phase 1: Static Vendoring

Goal:

- vendor the selected MiniMax skills into the repo
- no runtime use yet

Tasks:

- add `vendor/minimax-skills/`
- keep upstream structure intact
- document upstream source and version

### Phase 2: Internal Skill Loader

Goal:

- read vendored `SKILL.md` files from our own repo

Tasks:

- add a loader module in Rust
- parse skill file contents as plain text
- provide allowlist lookup by ID

Suggested file:

- `src-tauri/src/skills/vendored.rs`

### Phase 3: Prompt Injection

Goal:

- use vendored skills in selected code subtasks

Tasks:

- extend code prompt assembly
- inject vendored skill content only when selected
- add a local override rule

### Phase 4: Visibility

Goal:

- make vendored skill usage visible in tooling/logs

Tasks:

- emit an event when a vendored skill is selected
- show in UI or tool logs:
  - which skill was used
  - for which subtask

This is important because hidden prompt augmentation becomes hard to debug.

## Logging Recommendation

Add an event such as:

```text
vendored-skill-selected
```

Payload:

- subtask id
- mode
- skill id
- reason

This should be surfaced in either:

- Director progress messages
- Tool log panel
- Blackboard panel

## Compatibility Rules

Vendored skills must be treated as compatible only if they do **not** break these rules:

1. Do not replace shared blackboard communication.
2. Do not override our subtask boundaries.
3. Do not cause the agent to re-plan the entire project.
4. Do not instruct the agent to ignore `PLAN.md`.
5. Do not bypass Codex inline review in `code` mode.

If a vendored skill conflicts with any of the above, our local rules win.

## UI Recommendation

Eventually show vendored skill use in the interface.

Possible places:

- Blackboard panel:
  - "Current helper skill: frontend-dev"
- Tool logs:
  - "Vendored skill selected: frontend-dev"
- Subtask status:
  - "P2 using vendored skill frontend-dev"

This helps users trust that the system is deterministic rather than magical.

## Risks

### 1. Prompt Conflict

Some vendored skills may include their own workflow assumptions, for example:

- ask clarifying questions
- start from requirements gathering
- produce their own architecture plan

This can conflict with our already-decided mode and blackboard state.

Mitigation:

- only inject vendored skills in tightly bounded contexts
- add explicit local precedence rules

### 2. Prompt Bloat

Large `SKILL.md` files can blow up prompt size.

Mitigation:

- trim or preprocess vendored skills
- allow extracting only the useful sections
- avoid injecting the full skill when a short subset is enough

### 3. Hidden Behavior Drift

If vendored skills change upstream and we update blindly, system behavior may drift.

Mitigation:

- pin the vendored snapshot
- track source commit in a local metadata file
- review diffs before updating

## Update Policy

When vendoring third-party skills:

- record upstream repository URL
- record commit SHA
- record vendored date

Recommended local metadata file:

```text
vendor/minimax-skills/VENDORED_FROM.md
```

Suggested contents:

- upstream repo
- commit sha
- selected skills copied
- local modifications, if any

## Minimal First Milestone

The smallest worthwhile implementation is:

1. Vendor `frontend-dev` and `fullstack-dev`
2. Add a Rust loader for vendored skill text
3. In `code` mode, inject:
   - `frontend-dev` for `P*` subtasks
   - `fullstack-dev` for mixed UI/API subtasks
4. Add a local precedence rule
5. Emit a visible event when used

This gives real value without destabilizing the whole app.

## What Not To Do

Do not:

- symlink vendored skills into the user's global directories
- silently install into `~/.agents/skills`
- rely on Claude/Codex native auto-discovery for app correctness
- let vendored skills become the top-level orchestrator

## Recommended Next Step

Implement this in the following order:

1. vendor selected skill files
2. add `vendored.rs` loader
3. inject vendored skills in `code` mode only
4. log when a vendored skill is used
5. expand later to other modes only if proven helpful

## Concrete Initial File Targets

Likely new file:

- `src-tauri/src/skills/vendored.rs`

Likely modified files:

- `src-tauri/src/skills/mod.rs`
- `src-tauri/src/skills/code.rs`
- optionally `src-tauri/src/skills/plan.rs`
- `src/types.ts`
- `src/App.tsx` if UI visibility is added

## Final Recommendation

Yes, vendoring MiniMax skills into this repo is the correct direction.

But they should be integrated as:

- internal skill assets
- selected by our orchestrator
- injected into prompts under our control

They should **not** be treated as:

- a replacement for our coordination architecture
- a dependency on user-global skill installation
- an automatically trusted workflow engine
