# Gemini Frontend Task: Blackboard Panel

## Background

The app architecture has already been refactored away from direct agent-to-agent transcript passing.

Current backend/orchestration status:

- `plan` mode now uses a shared planning blackboard:
  - `PLAN_BLACKBOARD.md`
  - `PLAN_BLACKBOARD.json`
- `code` mode now uses a shared execution blackboard:
  - `BLACKBOARD.md`
  - `BLACKBOARD.json`
- The frontend already receives `blackboard-updated` events from Tauri.
- The frontend currently only shows blackboard progress as Director chat messages.

This means the architecture is now closer to a real multi-agent system, but the UI still feels like a chat wrapper.

The next step is to expose the blackboards as first-class UI, not just as incidental chat logs.

## Goal

Build a dedicated Blackboard panel in the left sidebar area so users can inspect:

- the planning blackboard
- the execution blackboard
- the currently active subtask
- the latest status transitions

The panel should feel intentional and operational, like a control room or mission board, not a generic markdown viewer.

## Product Intent

The UI should communicate this mental model:

- `plan` is a collaborative planning board
- `code` is a subtask execution board
- Claude and Codex coordinate through shared state
- chat is secondary
- blackboard state is primary

## Existing Relevant Files

Frontend:

- `src/App.tsx`
- `src/types.ts`
- `src/components/FileTreePanel.tsx`
- `src/components/ToolLogPanel.tsx`
- `src/components/HistoryPanel.tsx`

Backend / orchestration context:

- `src-tauri/src/skills/blackboard.rs`
- `src-tauri/src/skills/plan_board.rs`
- `src-tauri/src/skills/code.rs`
- `src-tauri/src/skills/plan.rs`
- `src-tauri/src/workspace.rs`

## Required UX

Add a new sidebar tab for Blackboard.

Expected behavior:

1. When a workspace is available, the user can open the Blackboard tab.
2. The Blackboard tab should display:
   - current workspace name
   - planning blackboard status
   - execution blackboard status
   - active subtask if any
   - a readable view of the latest board contents
3. When `blackboard-updated` events arrive, the panel should refresh automatically.
4. The panel should work even if only one board exists:
   - during early `plan` stage only `PLAN_BLACKBOARD.*` may exist
   - after `code` starts `BLACKBOARD.*` may also exist
5. If no board exists yet, show a clear empty state.

## Visual Direction

Avoid a generic file browser look.

Preferred direction:

- A dedicated operational dashboard feel
- Strong section labels
- Clear status pills
- Timeline / activity feel for recent updates
- Board switcher or stacked cards for:
  - Plan Board
  - Execution Board
- Markdown/body content can be rendered in a compact readable way, but should not dominate the whole panel

Suggested layout:

- Header:
  - `BLACKBOARD`
  - workspace label
  - refresh button
  - collapse button
- Top summary cards:
  - Plan Board status
  - Execution Board status
- Middle:
  - active board switcher or segmented control
- Main body:
  - rendered board content
- Bottom:
  - recent event timeline from `blackboard-updated`

## State / Data Needed

Current frontend already has:

- `workspace`
- `messages`
- `blackboard-updated` events

Need to add:

- blackboard panel state
- latest blackboard event list
- blackboard file content cache

## Important Backend Dependency

Right now the Tauri backend exposes:

- `workspace_tree`
- `open_project`
- `read_project_docs`

It does **not** currently expose a direct command to read arbitrary workspace files like:

- `PLAN_BLACKBOARD.md`
- `BLACKBOARD.md`
- `PLAN_BLACKBOARD.json`
- `BLACKBOARD.json`

So for a proper Blackboard panel, one of these must happen:

### Preferred

Add a new Tauri command such as:

- `read_workspace_file(path: string, relativePath: string) -> { content: string }`

Constraints:

- must only read files inside the selected workspace
- must reject path traversal
- should handle missing files gracefully

### Acceptable Fallback

If you do not want to add a generic reader, add a focused command:

- `read_blackboards(path: string) -> { planMarkdown?: string, planJson?: string, execMarkdown?: string, execJson?: string }`

If Gemini prefers to stay frontend-only, ask Codex to add this backend support first.

## Recommended Frontend Changes

### 1. Add New Sidebar Tab

In `src/App.tsx`:

- extend `activeSidebarTab` to include `blackboard`
- add a new activity bar button
- dock a new panel in the sidebar stack

### 2. Add Types

In `src/types.ts`:

- add blackboard panel data types
- add event timeline item type if needed

### 3. Create New Component

Create a new component:

- `src/components/BlackboardPanel.tsx`

Responsibilities:

- load blackboard files for the current workspace
- refresh on workspace change
- refresh on `blackboard-updated`
- render status summary
- render board content
- render recent event timeline

### 4. Content Strategy

Recommended:

- Prefer markdown file for human-readable display
- Optionally parse JSON file for top-level status pills and active subtask

Best UX:

- Use `.json` for structured summary
- Use `.md` for detailed readable content

## Required Quality Bar

The panel should:

- look distinct from Explorer and Tool Logs
- be usable on smaller laptop widths
- not feel visually noisy
- handle missing files cleanly
- not spam duplicate timeline entries on repeated renders

## Acceptance Criteria

The work is done when all of the following are true:

1. A Blackboard tab exists in the activity bar.
2. Opening it shows a dedicated panel, not a reused file tree.
3. During `plan`, the panel can show `PLAN_BLACKBOARD`.
4. During `code`, the panel can show `BLACKBOARD`.
5. The panel updates automatically when `blackboard-updated` fires.
6. The user can understand the current coordination state without reading chat messages.
7. Empty and partial states are graceful.

## Non-Goals

Do not:

- redesign the whole app
- replace chat
- remove Explorer or Tool Logs
- add overly complex graph visualizations

Keep it focused on making the blackboard architecture visible and credible.

## Nice-to-Have

If time allows:

- diff-like highlighting when board content changes
- status color mapping:
  - pending
  - in_progress
  - needs_fix
  - done
  - failed
- click-to-copy file names or subtask IDs

## Suggested Handoff Note

If Gemini is implementing this, it should first inspect:

- `src/App.tsx`
- `src/types.ts`
- `src/components/FileTreePanel.tsx`
- `src-tauri/src/skills/blackboard.rs`
- `src-tauri/src/skills/plan_board.rs`

and then decide whether it also wants the small backend file-read API added by Codex.
