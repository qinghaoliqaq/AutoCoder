---
name: spec-driven-implementation
label: Spec-Driven Implementation
category: planning
description: Orchestrate the full spec-then-build workflow for a non-trivial
  task — write a tech spec, get review, then implement step-by-step keeping
  spec and code in sync. Use when the user asks to "spec-drive this", "do
  this properly with a spec", or wants the full Warp-style discipline.
---

# Spec-Driven Implementation

This skill is the **entrypoint** for the spec-driven workflow. It does
not write code or specs itself — it sequences other skills and enforces
the gates between them.

## When To Use

Use when:
- The task is large enough that diving straight into code would risk
  rework (touches 3+ modules, has architectural choices, or affects
  shared infrastructure)
- The user explicitly asks for "spec-driven", "with a spec", or
  references the workflow by name
- You're unsure whether to spec or just code — err on the side of
  spec'ing if a wrong implementation would cost more than 30 minutes
  to redo

Don't use for:
- One-line fixes / typo corrections
- Routine refactors with obvious scope
- Exploration ("just look at how X works") — use direct tool calls

## The Three Phases

### Phase 1 — Spec

Invoke the **`write-tech-spec`** skill (via the `Skill` tool, or by
following its instructions inline if you've already loaded it).

Outcome: `specs/<id>/TECH.md` exists with all required sections,
file:line citations verified, and 80–150 lines.

Phase gate: **stop and surface the spec to the user before
implementing.** Do this by:
1. Telling the user where the spec was written.
2. Highlighting the most important Risks & Open Questions.
3. Asking via `AskUserQuestion` whether to proceed, revise, or abandon.

If the user requests revisions, loop back to `write-tech-spec` with
their feedback as additional input. Do NOT silently proceed to Phase 2
without explicit go-ahead.

### Phase 2 — Implement

Invoke the **`implement-specs`** skill against the now-approved spec.

Outcome: code changes committed in step-sized commits, spec updated
in the same commits whenever the implementation diverges from what
the spec proposed, optional PROJECT_LOG.md / DECISIONS.md maintained.

### Phase 3 — Verify & Document

After the last Implementation Plan step:
1. Run the full test scenarios in TECH.md's Testing section.
2. Invoke the **`verify`** skill for a build / test sanity sweep.
3. Append a final PROJECT_LOG.md entry: "spec landed, PRs: …".
4. Confirm TECH.md's Current State now describes the new state, not
   the pre-change state.

## Decision Points That Justify Pausing

You should stop and ask the user at any of these points:

| Trigger | Why ask |
|---|---|
| Spec drafted, before implementation | The spec is the cheap-to-change artifact; review here saves rework |
| Implementation reveals the proposed approach won't work | Don't silently switch strategies |
| A required dependency / migration / breaking change emerges that wasn't in the spec | Material scope expansion |
| Two or more reasonable choices that the spec didn't anticipate | Let the human pick architectural direction |

For each pause, use `AskUserQuestion` with concrete options when
possible. "Should I proceed with A, switch to B, or abandon?" beats
"What do you want me to do?"

## Anti-Patterns

- ❌ Writing the spec and then immediately implementing without
  surfacing it for review. The spec is the cheap artifact — review
  there.
- ❌ Treating the spec as decoration. If you write a spec and then
  ignore it during implementation, you've wasted the spec time AND
  built misaligned code.
- ❌ Letting the spec rot during implementation. Drift is a bug.
- ❌ Speccing trivial changes. The discipline has a cost; spend it
  on work that benefits.

## Output

The human-visible artifacts of a successful spec-driven run are:
1. `specs/<id>/TECH.md` (final state, matching what shipped)
2. One or more commits, each tagged with `<id>`
3. Optional `specs/<id>/PROJECT_LOG.md` and `DECISIONS.md`
4. A final summary message in chat: `<id>` shipped, links to commits,
   notable decisions surfaced for the user's awareness.

## Related Skills

- `write-tech-spec` — Phase 1 worker.
- `implement-specs` — Phase 2 worker.
- `verify` — Phase 3 build / test verification.
- `simplify` — optional Phase 3 review pass on the diffs.
