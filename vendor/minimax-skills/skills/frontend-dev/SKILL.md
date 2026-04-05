---
name: frontend-dev
description: |
  Lightweight packaged frontend implementation guide for AI Dev Hub.
  Use for UI-heavy subtasks such as pages, dashboards, forms, landing sections,
  component composition, responsive layout, interaction polish, and frontend
  integration work that should stay aligned with the local project codebase.
license: MIT
metadata:
  version: "2.0.0"
  category: frontend
---

# Frontend Dev

This vendored skill is intentionally self-contained. It is a compact execution guide
for frontend subtasks inside AI Dev Hub and does not depend on external reference
files, generators, templates, fonts, or media scripts.

## When To Use

Use this skill when the current subtask is primarily about:

- building or updating a screen, page, modal, form, table, or dashboard
- wiring UI state to existing APIs
- improving responsive layout, empty/loading/error states, and interaction quality
- refining visual hierarchy, typography, spacing, and motion

Do not use it as a reason to redesign unrelated parts of the app.

## Operating Rules

1. Read the local project first.
2. Follow the repo's existing framework, routing, state, and styling conventions.
3. Prefer modifying existing components over introducing parallel abstractions.
4. Keep the write scope bounded to the current subtask.
5. If backend changes are required, coordinate only through the project files and blackboard state, not assumptions.

## Implementation Checklist

- Confirm where the screen is mounted and how navigation reaches it.
- Identify the real data contract: props, loader, API response, validation, and error cases.
- Implement the happy path first, then loading, empty, error, and disabled states.
- Make the UI work on desktop and mobile without relying on fixed heights or brittle pixel math.
- Preserve accessibility basics: labels, focus order, keyboard reachability, visible focus, semantic buttons/links.

## UI Standards

- Prefer clear visual hierarchy over decorative effects.
- Reuse the local design language if one exists.
- Avoid placeholder copy when real product copy can be inferred from the feature.
- Avoid introducing new dependencies unless the project already uses them or the gain is clear.
- Use motion sparingly and only where it improves comprehension.

## Data And Integration Rules

- Treat API calls, auth state, and async transitions as failure-prone by default.
- Show pending state during async work.
- Surface actionable error messages instead of silent failure.
- Do not fake success locally when the backend contract is uncertain.
- If a field is required by the backend, validate it in the UI as well.

## Review Bar

The subtask is only ready for review when:

- the requested UI surface exists and is reachable
- data flow is wired correctly
- in-scope edge states are implemented
- the change matches the local project style closely enough to ship
- there are no obvious regressions in neighboring UI affected by the edit

## Delivery

At completion, summarize:

- what user-facing UI was added or changed
- what data/API wiring was implemented
- which files were touched
