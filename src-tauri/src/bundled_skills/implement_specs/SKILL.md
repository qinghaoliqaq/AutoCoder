---
name: implement-specs
label: Implement Specs
category: planning
description: Implement code changes from an existing specs/<id>/TECH.md, one
  Implementation Plan step per commit, and update the spec in the same PR
  whenever reality diverges. Use after write-tech-spec, or when the user
  points at an existing TECH.md and says "implement this".
---

# Implement Specs

This skill turns a `specs/<id>/TECH.md` into committed code. Its core
discipline:

> **The spec and the code update in the same PR.** A drifted spec is
> worse than no spec — it lies. Treat drift as a bug, not a normal
> outcome.

## When To Use

Use this skill when:

- A `specs/<id>/TECH.md` already exists and is approved (either by the
  user or by a previous `write-tech-spec` run)
- The user says "implement spec X" or "follow the plan in specs/Y/"
- You're picking up an in-progress spec implementation (PROJECT_LOG.md
  exists)

Do **not** start implementing without a spec — drop back to
`write-tech-spec` first, or implement directly if the work is too small
to spec.

## Operating Loop

For **each** numbered step in the spec's Implementation Plan:

1. **Re-read the spec.** It may have been updated since you last looked.
2. **Read the cited current-state code.** TECH.md cites `file:line`
   anchors; verify they still point where the spec claims. If the code
   has moved, update the spec citations *first*, then continue.
3. **Implement the step.** Stay scoped to what this step describes —
   don't drag in unrelated cleanup, refactors, or "while I'm here"
   improvements. Those belong in their own PRs.
4. **Run the testing scenarios** the spec lists for this step. If the
   spec doesn't list one, add the smallest test that exercises the new
   behavior end-to-end.
5. **Update the spec if reality diverged.** Common cases:
   - You discovered the proposed approach doesn't work → update Proposed
     Changes and Implementation Plan, then re-read step 1.
   - You found a missed dependency / risk → log it in
     `DECISIONS.md` (create if absent) with a one-line rationale.
   - You finished early and a planned step is no longer needed →
     strike it from the plan with a note explaining why.
6. **Commit.** Spec edits and code edits go in the same commit. Commit
   messages reference the spec id: `feat(<id>): <step summary>`.

## PROJECT_LOG.md Discipline

For multi-day or multi-PR specs, maintain `specs/<id>/PROJECT_LOG.md`:

```markdown
# Project Log: <id>

## YYYY-MM-DD
- Step N done — <one-line summary>. Diff: <files touched>.
- Discovered: <any surprise>.
- Next: <step N+1 or pending decision>.
```

One entry per work session. The log is for **future you** and **other
agents** — write it like a flight recorder, not a status report.

## DECISIONS.md (when used)

Append-only ADR-style log. Use when you make a non-obvious choice that
isn't fully captured in the spec:

```markdown
## YYYY-MM-DD — <decision title>
**Context:** <one paragraph>
**Options considered:** <bulleted>
**Choice:** <what & why>
**Consequence:** <what this means for future work>
```

Skip DECISIONS.md for routine implementation choices. Use it for
choices someone would later go "wait, why did we…?" about.

## Anti-Patterns

- ❌ Implementing all steps in one giant commit "to save time". You
  lose review granularity and bisectability.
- ❌ Silently changing the approach without updating the spec. The
  next agent reads the stale spec and is misled.
- ❌ Adding scope to a step ("I'll also fix this typo, this naming,
  this nearby bug…"). Open separate PRs.
- ❌ Skipping the testing scenario because "it's obviously correct."
  Specs that go unverified rot.
- ❌ Treating spec drift as a clerical chore at the end. By then
  you've forgotten what changed and why.

## Completion

A spec is complete when:

- All Implementation Plan items are checked off (or struck with
  rationale)
- Tests in the Testing section pass
- TECH.md's Current State now describes the *new* state (since this
  is what the spec landed)
- DECISIONS.md captures any non-obvious choices made along the way
- A final PROJECT_LOG.md entry summarizes "spec landed" and links the
  PRs

After completion, the spec stays in `specs/<id>/` as a record of how
this piece of the system came to be. Don't delete it — future agents
diagnosing related code use it for archeology.

## Related Skills

- `write-tech-spec` — produces the TECH.md this skill consumes.
- `spec-driven-implementation` — orchestrator that chains spec writing
  and implementation.
- `verify` — invoke at the end of each step to confirm the build still
  passes and tests still run.
