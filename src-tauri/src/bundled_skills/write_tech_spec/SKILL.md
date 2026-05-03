---
name: write-tech-spec
label: Write Tech Spec
category: planning
description: Draft a technical specification under specs/<id>/TECH.md before
  writing code. Use when the user asks for a "spec", "tech spec", or
  "implementation plan" — or when a task is large enough that getting the
  approach reviewed before coding is cheaper than writing the code twice.
---

# Write Tech Spec

This skill produces a technical specification grounded in the **current
codebase**, not a wish-list. The output is a markdown file at
`specs/<id>/TECH.md` that the team (or your future self) can review,
critique, and then hand back to an agent for implementation.

## When To Use

Use this skill when:

- The user explicitly asks for a spec / tech spec / design doc
- The task touches more than two modules and requires a plan before
  diving in
- A naive implementation would risk breaking other parts of the codebase
- You're about to make a non-trivial architectural decision and want it
  written down before you commit

Do **not** use this for trivial fixes (one-line changes, typos, single-
function refactors). Spec-writing has a fixed cost; spend it where it
actually saves rework.

## Spec Directory Layout

```
specs/<id>/
  TECH.md           ← this skill writes this
  PRODUCT.md        ← optional; product-level "what & why"
  PROJECT_LOG.md    ← optional; running log for multi-day work
  DECISIONS.md      ← optional; ADR-style decision records
```

`<id>` is one of:
- a GitHub issue id like `gh-1063`
- a ticket id like `APP-3076`
- a short kebab-case slug like `parallel-subtask-merge`

Pick one and use it consistently. If unsure, ask the user (via
`AskUserQuestion`) or default to a slug derived from the task title.

## TECH.md Required Sections

Every TECH.md must have these sections, in this order, with these exact
H2 headings:

```markdown
# <Title>

## Context
## Current State
## Proposed Changes
## End-to-End Flow
## Implementation Plan
## Testing
## Risks & Open Questions
```

### Section guidance

- **Context** (3–6 bullets) — Why are we doing this? What's the user-
  facing motivation? Link to the relevant PRODUCT.md or issue if one
  exists.

- **Current State** — Summarize how the codebase handles the relevant
  area today. **Every claim must be cited with a `file:line` reference.**
  Use Read / Grep to verify before citing. If you cite a file you didn't
  read, you've failed this skill.

  Example: `the agent loop dispatches tool calls one batch at a time
  (src-tauri/src/tools/mod.rs:467-583)`.

- **Proposed Changes** — Phased breakdown of code changes. Each phase
  should be independently reviewable and small enough to fit in a
  single PR. List the files you'll touch and the new files you'll
  create. Cite any unchanged code you depend on.

- **End-to-End Flow** — A walkthrough of one concrete scenario after
  the change is in place. "User does X → component A invokes B with
  Y → result Z." This catches integration gaps that section-by-
  section thinking misses.

- **Implementation Plan** — Numbered checklist the implementing agent
  follows. Each item is a discrete commit-sized step. Order matters:
  earlier steps unblock later ones.

- **Testing** — What proves the changes work? List specific test files
  to add or modify, plus the manual / integration scenarios that aren't
  covered by automated tests.

- **Risks & Open Questions** — What might go wrong? What did you
  consider and reject? What requires user input before implementation?

## Length Target

Aim for **80–150 lines**. Specs longer than 200 lines almost always
mean the scope is too big — split them or push detail down into
linked subspecs. Specs under 50 lines usually mean the work didn't
need a spec.

## Citation Discipline

This is the most important rule. **Read code before citing it.**

- ✅ `the resolver dedupes by SkillProvider::rank
  (src-tauri/src/bundled_skills/loader.rs:117)` — actually verified.
- ❌ `the resolver probably dedupes somewhere in loader.rs` — vague,
  unactionable, often wrong.

If you can't cite a file:line for a claim about current behavior, run
Grep / Read first. A spec built on guesses is worse than no spec.

## Output

Write the spec to `specs/<id>/TECH.md` using the Write tool. Do not
print it inline in chat — the spec lives in git, not in conversation
history.

After writing, summarize in 2–3 sentences:
- the chosen `<id>`,
- the path you wrote,
- one open question or risk that the user should respond to before
  implementation begins.

## Related Skills

- `implement-specs` — picks up where this leaves off; reads the TECH.md
  back and translates each Implementation Plan item into commits.
- `spec-driven-implementation` — orchestrator that chains
  `write-tech-spec` → review → `implement-specs`.
