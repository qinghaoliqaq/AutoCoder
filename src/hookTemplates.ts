/**
 * Curated hook templates exposed via the "Browse templates" picker in the
 * Hooks settings tab. Each template is a one-click starting point — users
 * can edit matcher/command/timeout after inserting.
 *
 * Conventions:
 *   * Commands assume Unix shell (`sh -c`). On Windows the user may need
 *     to translate; templates with a `platform_note` flag the constraint.
 *   * Templates that read structured fields from the JSON payload use
 *     `jq`. Where `jq` isn't available, the env-var fallback
 *     (`AUTOCODER_TOOL_NAME` / `AUTOCODER_AGENT_ID` / `AUTOCODER_WORKSPACE`)
 *     is also documented in `config.example.toml`.
 *   * Timeouts default to a short value (5–30s) so a template hook can't
 *     wedge an agent for the full 5-minute cap if something goes wrong.
 */

import type { HookEntry, HookEvent } from './types';

export interface HookTemplate {
  /** Stable id; used as the React key and for picker filtering. */
  id: string;
  name: string;
  description: string;
  event: HookEvent;
  /** The actual hook entry to insert. */
  entry: HookEntry;
  /** Optional caveat shown beneath the description. */
  platform_note?: string;
}

export const HOOK_TEMPLATES: readonly HookTemplate[] = [
  {
    id: 'block-rm-rf',
    name: 'Block rm -rf and sudo in Bash',
    description:
      'Refuse Bash dispatches whose command looks like a recursive force-delete or a privilege escalation.',
    event: 'pre_tool_use',
    entry: {
      matcher: 'Bash',
      command: `cmd=$(jq -r '.tool_input.command' 2>/dev/null)
case "$cmd" in
  *"rm -rf /"*|*"rm -fr /"*|*"sudo "*|*":(){"*)
    echo "blocked: dangerous command pattern" >&2
    exit 1
    ;;
esac`,
      timeout_secs: 5,
    },
    platform_note: 'Requires jq for JSON-payload parsing.',
  },
  {
    id: 'cargo-fmt-after-rust-edit',
    name: 'cargo fmt --check after Rust edits',
    description:
      'When the agent edits a .rs file, surface formatter drift back to the model so it can fix it next round.',
    event: 'post_tool_use',
    entry: {
      matcher: 'Edit',
      // NOTE: avoid `cargo fmt --check 2>&1 | head -20` — piping into
      // head masks cargo's exit code (head's success becomes the
      // pipeline status), so drift wouldn't surface as a non-zero
      // exit. Instead capture both streams to a temp var and emit
      // them on drift only.
      command: `path=$(jq -r '.tool_input.file_path' 2>/dev/null)
case "$path" in
  *.rs)
    cd "$AUTOCODER_WORKSPACE" || exit 0
    out=$(cargo fmt --check 2>&1) && exit 0
    echo "$out" | head -20
    exit 1
    ;;
esac`,
      timeout_secs: 30,
    },
  },
  {
    id: 'prettier-after-js-edit',
    name: 'prettier --write after JS/TS edits',
    description:
      'Auto-format JS/TS/JSON/MD files immediately after an Edit so the agent reads the canonical form on the next file Read.',
    event: 'post_tool_use',
    entry: {
      matcher: 'Edit',
      // `--no` (alias for --no-install) makes npx fail rather than
      // silently install prettier on first use. Don't combine with
      // `-y`: that flag would auto-confirm an install, contradicting
      // --no, and recent npx versions warn or skip in the conflict.
      command: `path=$(jq -r '.tool_input.file_path' 2>/dev/null)
case "$path" in
  *.ts|*.tsx|*.js|*.jsx|*.json|*.md)
    npx --no prettier --write "$path" 2>/dev/null && echo "prettier: formatted $path"
    ;;
esac`,
      timeout_secs: 20,
    },
  },
  {
    id: 'log-tool-calls',
    name: 'Append every tool call to a log file',
    description:
      'Audit trail: writes one timestamped line per tool dispatch to .autocoder/tool-calls.log under the workspace.',
    event: 'pre_tool_use',
    entry: {
      matcher: '*',
      command: `mkdir -p "$AUTOCODER_WORKSPACE/.autocoder" 2>/dev/null
ts=$(date -u +%Y-%m-%dT%H:%M:%SZ)
echo "$ts $AUTOCODER_AGENT_ID $AUTOCODER_TOOL_NAME" >> "$AUTOCODER_WORKSPACE/.autocoder/tool-calls.log"`,
      timeout_secs: 5,
    },
  },
  {
    id: 'block-edit-outside-workspace',
    name: 'Block edits to files outside the workspace',
    description:
      'Defense in depth: refuses Edit / Write calls whose target path resolves outside the active workspace.',
    event: 'pre_tool_use',
    entry: {
      matcher: 'Edit',
      command: `path=$(jq -r '.tool_input.file_path' 2>/dev/null)
case "$path" in
  /*) ;;
  *) exit 0 ;;
esac
case "$path" in
  "$AUTOCODER_WORKSPACE"/*) ;;
  *) echo "blocked: file_path is outside the workspace ($path)" >&2; exit 1 ;;
esac`,
      timeout_secs: 5,
    },
    platform_note: 'Requires jq. Apply the same pattern with matcher="Write" for symmetry.',
  },
  {
    id: 'desktop-notify-stop',
    name: 'Desktop notification when run finishes',
    description:
      "Pings the OS notification service when a top-level agent run completes, so you don't need to keep the window focused.",
    event: 'stop',
    entry: {
      matcher: '*',
      command: `if command -v notify-send >/dev/null 2>&1; then
  notify-send 'AutoCoder' 'Agent run finished'
elif command -v osascript >/dev/null 2>&1; then
  osascript -e 'display notification "Agent run finished" with title "AutoCoder"'
fi`,
      timeout_secs: 5,
    },
    platform_note: 'Linux uses notify-send; macOS uses osascript. No-op if neither is on PATH.',
  },
  {
    id: 'git-status-on-stop',
    name: 'Print git status when run finishes',
    description:
      'Quick "what did the agent change?" recap — printed to logs when the top-level run ends.',
    event: 'stop',
    entry: {
      matcher: '*',
      command: `cd "$AUTOCODER_WORKSPACE" || exit 0
if [ -d .git ] || git rev-parse --git-dir >/dev/null 2>&1; then
  git status --short --branch
fi`,
      timeout_secs: 10,
    },
  },
];

/** Lookup by id; useful for tests and for the picker's keyed list. */
export function findTemplate(id: string): HookTemplate | undefined {
  return HOOK_TEMPLATES.find((t) => t.id === id);
}
