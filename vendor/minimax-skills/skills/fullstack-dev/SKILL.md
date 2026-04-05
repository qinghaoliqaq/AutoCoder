---
name: fullstack-dev
description: |
  Lightweight packaged full-stack implementation guide for AI Dev Hub.
  Use for subtasks that cross frontend and backend boundaries, such as API-backed
  features, auth flows, CRUD modules, data synchronization, validation, and
  service integration that must be completed end-to-end.
license: MIT
metadata:
  version: "2.0.0"
  category: full-stack
---

# Fullstack Dev

This vendored skill is intentionally self-contained. It is a compact execution guide
for API-backed feature work inside AI Dev Hub and does not depend on bundled
reference documents.

## When To Use

Use this skill when the current subtask requires both sides of the system, for example:

- adding or updating an API endpoint and the UI that consumes it
- implementing authentication, authorization, or session-dependent features
- shipping CRUD flows that span schema, handlers, client calls, and UI states
- wiring file upload, background processing, or realtime updates into the product

## Core Rules

1. Read the existing code paths end to end before editing.
2. Keep the scope anchored to the current subtask; do not expand into a full rewrite.
3. Prefer existing architectural patterns over introducing a second style.
4. Make backend and frontend contracts explicit in code, not implied in prose.
5. If a migration, config change, or dependency is required, apply the minimal robust change.

## Backend Standards

- Validate input at the boundary.
- Return consistent success and error shapes.
- Do not hide failures with overly broad catch blocks.
- Keep business logic out of transport-layer glue where the project already separates them.
- Preserve auth and permission checks on every protected path.

## Frontend Standards

- Reflect backend validation and auth states in the UI.
- Handle loading, empty, success, and error states explicitly.
- Do not assume stale cached data is correct after a write; refresh or reconcile it.
- Keep form state, optimistic updates, and retries understandable and bounded.

## Integration Checklist

- Confirm route or handler registration.
- Confirm request payload and response shape.
- Confirm the client call site and state update path.
- Confirm in-scope permission behavior.
- Confirm not-found, invalid-input, and duplicate/conflict behavior where relevant.

## Review Bar

The subtask is only ready for review when:

- the feature works across the backend/frontend boundary
- validation and error handling are present on both sides where needed
- auth and access control remain correct
- the implementation is integrated into the actual user flow, not left orphaned
- touched files are limited to what the subtask really needs

## Delivery

At completion, summarize:

- which backend behavior changed
- which frontend behavior changed
- any contract, config, or migration implications
- which files were touched
