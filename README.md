# AutoCoder

AutoCoder is an AI-powered desktop coding workspace for end-to-end software delivery. It combines a Tauri desktop shell, a React frontend, and a Rust orchestration backend to coordinate planning, coding, debugging, review, and testing across multiple agents.

This repository is currently an early-stage open-source project. The internal product name `AI Dev Hub` still appears in parts of the UI and codebase.

## What This Project Is For

Most AI coding tools are optimized for one agent in one chat. AutoCoder is built around a different idea: software delivery is a workflow, not a single prompt.

This project tries to make that workflow explicit:

- A `Director` decides which skill should run next
- `plan`, `code`, `debug`, `review`, and `test` are treated as separate execution modes
- Claude and Codex are used for different responsibilities
- shared blackboards (`PLAN_BLACKBOARD.md`, `BLACKBOARD.md`, `BLACKBOARD.json`) act as the coordination layer
- project files, docs, history, tool logs, and session state stay visible in the desktop app

In practice, AutoCoder is trying to be a local AI software delivery console rather than a plain chat wrapper.

## Core Features

- Multi-stage workflow orchestration with a Director layer
- Dedicated modes for planning, coding, debugging, review, and testing
- Shared blackboard collaboration instead of loose agent-to-agent chat
- Workspace creation, file tree exploration, and project document ingestion
- Session history with restorable Director context
- Tool call logging for Claude and Codex runs
- Inline subtask review loop: Claude implements, Codex reviews, Claude repairs
- Support for vendored third-party skills injected at runtime
- Configurable Director backend through OpenAI-compatible or Anthropic-compatible APIs
- Desktop UI built with Tauri, React, TypeScript, and Tailwind CSS

## How It Works

### 1. Director

The Director is the traffic controller. It reads the user's input and decides whether the app should:

- chat normally
- create a plan
- start implementation
- debug a problem
- review the codebase
- run tests

The Director does not implement features itself. It routes work to the right skill.

### 2. Skills

The backend organizes execution into separate skills:

- `plan`: creates or reviews a technical plan
- `code`: implements subtasks against `PLAN.md` using a shared blackboard loop
- `debug`: focuses on fault isolation and repair
- `review`: checks implementation coverage, security, and cleanup
- `test`: generates test plans, runs integration checks, and writes project reports

### 3. Shared Blackboards

Instead of letting agents coordinate through hidden conversation alone, AutoCoder writes structured coordination state into project files. That makes the workflow more inspectable and easier to recover.

Typical files:

- `PLAN.md`
- `PLAN_BLACKBOARD.md`
- `BLACKBOARD.md`
- `BLACKBOARD.json`
- `change.log`
- `bugs.md`

### 4. Runners

The Rust backend shells out to local CLIs for execution:

- `claude`
- `codex`

These runners stream tokens back into the UI, record tool activity, and support cancellation.

## Tech Stack

- Desktop shell: Tauri 2
- Frontend: React 19 + TypeScript + Vite
- Styling: Tailwind CSS
- Backend: Rust + Tokio + Reqwest
- Testing: Vitest and Rust unit tests

## Repository Structure

```text
.
├── src/                     # React UI
├── src-tauri/               # Rust backend and Tauri app
│   ├── src/
│   │   ├── director.rs      # Director orchestration
│   │   ├── history.rs       # Session persistence
│   │   ├── workspace.rs     # Workspace and file access
│   │   └── skills/          # plan/code/debug/review/test runners
│   └── prompts/             # Prompt templates used by the backend
├── vendor/                  # Vendored third-party skills
├── config.example.toml      # Example Director configuration
└── VENDORED_SKILLS_ARCHITECTURE.md
```

## Prerequisites

Before running the app locally, you need:

- Node.js 18+
- Rust toolchain
- Tauri system prerequisites for your OS
- `claude` CLI installed and available in `PATH`
- `codex` CLI installed and available in `PATH`
- A Director model endpoint configured through `config.toml` or environment variables

## Configuration

Copy the example config:

```bash
cp config.example.toml config.toml
```

Then fill in your Director settings:

```toml
[director]
api_key    = "your-api-key"
base_url   = "https://api.openai.com/v1"
model      = "gpt-4o"
api_format = "openai"
```

Supported API wire formats:

- `openai`
- `anthropic`

The app also supports environment variable overrides:

- `DIRECTOR_API_KEY`
- `DIRECTOR_BASE_URL`
- `DIRECTOR_MODEL`
- `DIRECTOR_API_FORMAT`

## Local Development

Install JavaScript dependencies:

```bash
npm install
```

Start the frontend dev server:

```bash
npm run dev
```

Start the desktop app:

```bash
npm run tauri dev
```

Build the frontend bundle:

```bash
npm run build
```

Run frontend unit tests:

```bash
npm test
```

Run Rust tests:

```bash
cd src-tauri
cargo test
```

## Current Status

AutoCoder is usable as a serious prototype, but it is still in active development.

Current characteristics:

- the architecture is already modular and real
- the orchestration model is the main value of the project
- parts of the naming still reflect the earlier `AI Dev Hub` identity
- the repository is not yet fully polished for broad public consumption

If you are evaluating this project, treat it as an open-source alpha rather than a finished product.

## Design Principles

- Workflow over chat
- Coordination over hidden agent magic
- Local visibility over black-box execution
- Inspectable project state over ephemeral prompts
- Human-readable artifacts over opaque internal memory

## Vendored Skills

This repository includes a vendored skill architecture for selectively reusing external skill assets without relying on global agent skill discovery.

Key rule:

- vendored skills are implementation aids
- the local orchestrator and blackboard files remain the source of truth

See [VENDORED_SKILLS_ARCHITECTURE.md](./VENDORED_SKILLS_ARCHITECTURE.md) for the design rationale.

## Contributing

Contributions are welcome, especially in these areas:

- workflow reliability
- UI polish and information architecture
- test stability
- prompt design and skill contracts
- platform compatibility
- documentation

If you open an issue or PR, include:

- what you expected
- what actually happened
- your environment
- whether the issue is in `plan`, `code`, `debug`, `review`, or `test`

## License

This project is licensed under the Apache License 2.0. See `LICENSE`.
