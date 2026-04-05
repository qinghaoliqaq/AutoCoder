/**
 * Agent SDK Sidecar — bridges the Rust Tauri backend to Claude Agent SDK.
 *
 * Protocol: line-delimited JSON over stdin/stdout.
 *
 * Request (one JSON object per line on stdin):
 *   {
 *     "id":     "req-1",            // unique request ID
 *     "action": "query",            // "query" | "cancel" | "ping"
 *     "prompt": "Fix the bug...",   // for "query"
 *     "options": {                  // optional overrides
 *       "cwd":             "/path/to/workspace",
 *       "allowedTools":    ["Read", "Edit", "Glob", "Grep", "Bash"],
 *       "permissionMode":  "acceptEdits",
 *       "systemPrompt":    "You are a senior developer..."
 *     }
 *   }
 *
 * Response (one JSON object per line on stdout):
 *   { "id": "req-1", "type": "chunk",  "agent": "claude", "text": "..." }
 *   { "id": "req-1", "type": "tool",   "tool": "Edit", "input": "src/main.rs" }
 *   { "id": "req-1", "type": "result", "text": "...", "ok": true }
 *   { "id": "req-1", "type": "error",  "message": "..." }
 */

import { createInterface } from "node:readline";

// ── Lazy SDK import ──────────────────────────────────────────────────────────
// The Agent SDK is loaded lazily so the sidecar can start and respond to "ping"
// even if the SDK is not yet installed (allows graceful degradation).
let _query = null;
async function getQuery() {
  if (!_query) {
    try {
      const sdk = await import("@anthropic-ai/claude-agent-sdk");
      _query = sdk.query;
    } catch (err) {
      throw new Error(
        `Agent SDK not available: ${err.message}. Run "npm install" in agent-sidecar/`
      );
    }
  }
  return _query;
}

// ── Active queries (for cancellation) ────────────────────────────────────────
const activeQueries = new Map(); // id → AbortController

// ── Output helper ────────────────────────────────────────────────────────────
function send(obj) {
  process.stdout.write(JSON.stringify(obj) + "\n");
}

// ── Handle a single request ──────────────────────────────────────────────────
async function handleRequest(req) {
  const { id, action } = req;

  if (action === "ping") {
    send({ id, type: "result", text: "pong", ok: true });
    return;
  }

  if (action === "cancel") {
    const controller = activeQueries.get(req.targetId || id);
    if (controller) {
      controller.abort();
      activeQueries.delete(req.targetId || id);
    }
    send({ id, type: "result", text: "cancelled", ok: true });
    return;
  }

  if (action !== "query") {
    send({ id, type: "error", message: `Unknown action: ${action}` });
    return;
  }

  // ── Run Agent SDK query ────────────────────────────────────────────────────
  const abort = new AbortController();
  activeQueries.set(id, abort);

  try {
    const query = await getQuery();
    const opts = req.options || {};

    const agentQuery = query({
      prompt: req.prompt,
      abortSignal: abort.signal,
      options: {
        cwd: opts.cwd || process.cwd(),
        allowedTools: opts.allowedTools || [
          "Read", "Edit", "Write", "Glob", "Grep", "Bash",
        ],
        permissionMode: opts.permissionMode || "acceptEdits",
        systemPrompt: opts.systemPrompt || undefined,
        model: opts.model || undefined,
      },
    });

    let fullText = "";

    for await (const message of agentQuery) {
      if (abort.signal.aborted) break;

      // Assistant text chunks
      if (message.type === "assistant" && message.message?.content) {
        for (const block of message.message.content) {
          if ("text" in block && block.text) {
            fullText += block.text;
            send({ id, type: "chunk", agent: "claude", text: block.text });
          } else if ("name" in block) {
            // Tool call starting
            const input =
              typeof block.input === "string"
                ? block.input
                : JSON.stringify(block.input || {});
            send({
              id,
              type: "tool",
              tool: block.name,
              input: input.slice(0, 200),
            });
          }
        }
      }

      // Result / completion
      if (message.type === "result") {
        send({
          id,
          type: "result",
          text: fullText || message.result || "",
          subtype: message.subtype || "success",
          ok: true,
        });
      }
    }

    // If loop ended without a result message, send one
    if (!abort.signal.aborted) {
      send({ id, type: "result", text: fullText, ok: true });
    }
  } catch (err) {
    if (err.name === "AbortError" || abort.signal.aborted) {
      send({ id, type: "result", text: "cancelled", ok: false });
    } else {
      send({ id, type: "error", message: err.message || String(err) });
    }
  } finally {
    activeQueries.delete(id);
  }
}

// ── stdin reader ─────────────────────────────────────────────────────────────
const rl = createInterface({ input: process.stdin, terminal: false });

rl.on("line", (line) => {
  const trimmed = line.trim();
  if (!trimmed) return;
  try {
    const req = JSON.parse(trimmed);
    if (!req.id || !req.action) {
      send({ id: req.id || "?", type: "error", message: "Missing id or action" });
      return;
    }
    // Fire and forget — each request runs concurrently
    handleRequest(req).catch((err) => {
      send({ id: req.id, type: "error", message: err.message || String(err) });
    });
  } catch (err) {
    send({ id: "?", type: "error", message: `Invalid JSON: ${err.message}` });
  }
});

rl.on("close", () => {
  // Parent process closed stdin — shut down gracefully
  for (const [, controller] of activeQueries) {
    controller.abort();
  }
  process.exit(0);
});

// Signal readiness
send({ id: "init", type: "result", text: "sidecar ready", ok: true });
