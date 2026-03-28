You are Claude, the lead architect. The planning debate is now complete.

IMPORTANT: Write PLAN.md to `{{plan_path}}` using your Write and Edit tools.
Before synthesizing, read the shared planning blackboard at `{{plan_board_path}}`.
Treat that blackboard as the definitive debate record instead of relying on direct transcript handoff.
Each single Write or Edit call must contain AT MOST ~2000 characters of new content.
If a section would exceed 2000 characters, split it across multiple Edit calls.
Build the document in this order:
  1. Write: create the file with all section headings as a skeleton (headings only, ≤ 200 chars).
  2. Edit each section in order: Architecture → Key Design Decisions → Tech Stack →
     Backend Features → UI Screens / Views → File Structure → API Contract →
     Build & Run → Implementation Order.
  3. If any single section exceeds ~2000 characters, split it across multiple consecutive Edits.
After ALL edits are done, output exactly one line and nothing else:
PLAN_COMPLETE

Produce the definitive **PLAN.md** for this project. It must be:
- Complete enough that a developer can implement everything without referring back to the debate
- Structured so an automated reviewer can check each item off as implemented
- Preserve ALL technical decisions and their rationale from the debate — do not discard nuance

## Output format (use exactly these sections)

---

# PLAN.md — {one-line project name}

## Architecture
Describe the chosen architecture in full. Include: overall pattern, platform, data flow, key
design decisions from the debate, and WHY each major choice was made over the alternatives.
Write as many sentences as needed — do not truncate to hit an artificial limit.

## Key Design Decisions
Capture the most important trade-offs resolved during the debate. One decision per bullet:
- **Decision**: what was decided — **Why**: the reason, including what alternative was rejected and why.
(Include all decisions that affect implementation — auth strategy, storage approach, async handling, etc.)

## Tech Stack
| Layer | Choice |
|-------|--------|
| Backend | exact framework + version + database |
| UI | exact framework + version + key UI libs |
| Auth | strategy (JWT / session / none) + specifics |
| Communication | REST / GraphQL / local IPC / etc. |
(Add rows as needed for: job queue, caching, storage, search, etc.)

## Backend Features
One line per feature. Each line must be specific enough for a developer to implement without guessing.
- [ ] **F1. Feature name** — HTTP method + path; what it does; key constraints, validation rules, error cases
- [ ] **F2. Feature name** — ...
(No artificial word limit. Include every constraint, algorithm, or business rule that matters.)

## UI Screens / Views
One line per screen or major view. Be specific about interactions and data shown.
- [ ] **P1. Screen name** — what the user sees; key actions; what API calls it makes; edge cases handled
- [ ] **P2. Screen name** — ...
(For CLI: list commands + flags. For desktop: list windows/dialogs.)

## File Structure
ASCII tree showing both backend and UI directories. No explanations — just the tree.

## API Contract
(Omit this section if there is no network API.)
| Method | Path | Auth | Request body | Response |
|--------|------|------|-------------|---------|
One row per endpoint.

## Build & Run
Exact shell commands to install deps and start the project locally. No placeholder comments.
```bash
# Backend
<exact commands>

# UI / Frontend / App
<exact commands>

# Open / run
<URL or run command>
```

## Implementation Order
Numbered list. Each item = one concrete file or action. Max 20 steps.
Follow this sequence: data models → backend routes → UI scaffold → screens/pages → wire API → styling.

---

Now write the PLAN.md using the format above. When done, output exactly: PLAN_COMPLETE

Task: {{task}}
