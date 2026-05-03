/**
 * HooksTab — Hooks editor inside the Settings modal.
 *
 * Loads the current `[hooks]` section via `get_hooks_config`, lets the
 * user add / edit / delete entries per event, and saves back via
 * `save_hooks_config`. Has its own load + save lifecycle independent
 * of the General/Agent draft so the two can't clobber each other.
 *
 * Pure helpers (`emptyEntry`, `validateEntry`, `cleanForSave`) are
 * exported so the panel logic can be unit-tested without rendering.
 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  AlertCircle,
  CheckCircle2,
  LoaderCircle,
  Plus,
  Save,
  Sparkles,
  Trash2,
} from 'lucide-react';
import type { HookEntry, HookEvent, HooksConfig } from '../types';
import type { HookTemplate } from '../hookTemplates';
import HookTemplatePicker from './HookTemplatePicker';

// ── Pure helpers ─────────────────────────────────────────────────────────────

/** A blank hook entry. Default matcher `*` matches all tools. */
export function emptyEntry(): HookEntry {
  return { matcher: '*', command: '', timeout_secs: null };
}

/**
 * Validate a single entry. Returns `null` when the entry would round-
 * trip cleanly through serde, or a human-readable reason when not.
 *
 * Errors short-circuit save — the user fixes the entry before the
 * frontend even hits Rust, which keeps the error message close to the
 * field that caused it.
 */
export function validateEntry(entry: HookEntry, event: HookEvent): string | null {
  if (!entry.command.trim()) {
    return 'Command is required.';
  }
  // matcher only meaningful for tool-use events; stop ignores it.
  if (event !== 'stop' && !entry.matcher.trim()) {
    return 'Matcher is required (use "*" to match all tools).';
  }
  if (
    entry.timeout_secs !== null &&
    (entry.timeout_secs < 1 || entry.timeout_secs > 300)
  ) {
    return 'Timeout must be between 1 and 300 seconds.';
  }
  return null;
}

/**
 * Trim user-typed strings and force the matcher field to `"*"` for
 * stop hooks (where the backend ignores it but a stale value would be
 * confusing on next load).
 */
export function cleanForSave(config: HooksConfig): HooksConfig {
  const clean = (entry: HookEntry, event: HookEvent): HookEntry => ({
    matcher: event === 'stop' ? '*' : entry.matcher.trim() || '*',
    command: entry.command.trim(),
    timeout_secs: entry.timeout_secs,
  });
  return {
    pre_tool_use: config.pre_tool_use.map((e) => clean(e, 'pre_tool_use')),
    post_tool_use: config.post_tool_use.map((e) => clean(e, 'post_tool_use')),
    stop: config.stop.map((e) => clean(e, 'stop')),
  };
}

const EVENT_LABELS: Record<HookEvent, { title: string; subtitle: string }> = {
  pre_tool_use: {
    title: 'PreToolUse',
    subtitle:
      'Fires before each tool call. Non-zero exit blocks the call.',
  },
  post_tool_use: {
    title: 'PostToolUse',
    subtitle:
      "Fires after a tool returns. Hook stdout is appended to the model's view.",
  },
  stop: {
    title: 'Stop',
    subtitle:
      'Fires once the top-level agent run finishes. Subtasks do not trigger Stop.',
  },
};

// ── Component ────────────────────────────────────────────────────────────────

export default function HooksTab() {
  const [config, setConfig] = useState<HooksConfig | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [savedAt, setSavedAt] = useState<number | null>(null);
  const [dirty, setDirty] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);

  useEffect(() => {
    let cancelled = false;
    invoke<HooksConfig>('get_hooks_config')
      .then((cfg) => {
        if (!cancelled) {
          setConfig(cfg);
          setDirty(false);
        }
      })
      .catch((err) => {
        if (!cancelled) setLoadError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const updateEvent = useCallback(
    (event: HookEvent, next: HookEntry[]) => {
      setConfig((prev) => {
        if (!prev) return prev;
        return { ...prev, [event]: next };
      });
      setDirty(true);
      setSavedAt(null);
    },
    [],
  );

  const insertTemplate = useCallback((tpl: HookTemplate) => {
    setConfig((prev) => {
      if (!prev) return prev;
      // Clone the entry — the template itself is shared `readonly`
      // module-level data; without the clone, edits in the form would
      // mutate it across the whole app.
      const next = { ...tpl.entry };
      return { ...prev, [tpl.event]: [...prev[tpl.event], next] };
    });
    setDirty(true);
    setSavedAt(null);
  }, []);

  // First validation error across all events, or null if everything's clean.
  const firstError = useMemo(() => {
    if (!config) return null;
    const events: HookEvent[] = ['pre_tool_use', 'post_tool_use', 'stop'];
    for (const ev of events) {
      for (let i = 0; i < config[ev].length; i++) {
        const err = validateEntry(config[ev][i], ev);
        if (err) return { event: ev, index: i, message: err };
      }
    }
    return null;
  }, [config]);

  const onSave = async () => {
    if (!config || saving) return;
    setSaving(true);
    setSaveError(null);
    try {
      const payload = cleanForSave(config);
      await invoke('save_hooks_config', { hooks: payload });
      setConfig(payload);
      setDirty(false);
      setSavedAt(Date.now());
    } catch (err) {
      setSaveError(String(err));
    } finally {
      setSaving(false);
    }
  };

  if (loadError) {
    return (
      <div className="rounded-lg border border-rose-500/20 bg-rose-500/10 px-4 py-3 text-xs leading-5 text-rose-600">
        Failed to load hooks: {loadError}
      </div>
    );
  }
  if (!config) {
    return (
      <div className="flex items-center gap-2 p-4 text-xs text-content-tertiary">
        <LoaderCircle className="h-4 w-4 animate-spin" />
        Loading hooks…
      </div>
    );
  }

  const events: HookEvent[] = ['pre_tool_use', 'post_tool_use', 'stop'];

  return (
    <div className="flex flex-col gap-6">
      <div className="rounded-lg border border-edge-primary/30 bg-surface-tertiary/30 px-4 py-3 text-[12px] leading-relaxed text-content-secondary">
        Hooks are shell commands fired at agent lifecycle events. Each
        hook runs via <code>sh -c</code> on Unix or <code>cmd /C</code>{' '}
        on Windows, receives a JSON payload on stdin, and inherits the
        workspace as its working directory. See <code>config.example.toml</code>{' '}
        for env vars and payload shape.
      </div>

      <button
        onClick={() => setPickerOpen(true)}
        className="flex items-center justify-center gap-1.5 rounded-lg border border-themed-accent/40
                   bg-themed-accent/5 px-3 py-2 text-[11.5px] font-semibold text-themed-accent-text
                   transition-colors hover:bg-themed-accent/10"
      >
        <Sparkles className="h-3.5 w-3.5" />
        Browse templates
      </button>

      {events.map((event) => (
        <HookSection
          key={event}
          event={event}
          entries={config[event]}
          onChange={(next) => updateEvent(event, next)}
        />
      ))}

      {firstError && (
        <div className="flex items-start gap-2 rounded-lg border border-rose-500/20 bg-rose-500/10 px-4 py-2.5 text-xs text-rose-700 dark:text-rose-400">
          <AlertCircle className="mt-0.5 h-3.5 w-3.5 flex-shrink-0" />
          <span>
            <strong>{EVENT_LABELS[firstError.event].title} #{firstError.index + 1}:</strong>{' '}
            {firstError.message}
          </span>
        </div>
      )}
      {saveError && (
        <div className="flex items-start gap-2 rounded-lg border border-rose-500/20 bg-rose-500/10 px-4 py-2.5 text-xs text-rose-700 dark:text-rose-400">
          <AlertCircle className="mt-0.5 h-3.5 w-3.5 flex-shrink-0" />
          <span>{saveError}</span>
        </div>
      )}

      <div className="flex items-center justify-between border-t border-edge-primary/20 pt-4">
        <div className="text-[11px] text-content-tertiary">
          {savedAt && !dirty && (
            <span className="flex items-center gap-1.5 text-emerald-600 dark:text-emerald-400">
              <CheckCircle2 className="h-3.5 w-3.5" />
              Hooks saved.
            </span>
          )}
          {dirty && !savedAt && <span>Unsaved changes.</span>}
        </div>
        <button
          onClick={onSave}
          disabled={!dirty || saving || firstError !== null}
          className="flex items-center gap-1.5 rounded-lg bg-themed-accent/90 px-4 py-1.5 text-[12px] font-semibold
                     text-white transition-all hover:bg-themed-accent active:scale-[0.98]
                     disabled:cursor-not-allowed disabled:opacity-40"
        >
          {saving ? (
            <>
              <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
              Saving…
            </>
          ) : (
            <>
              <Save className="h-3.5 w-3.5" />
              Save hooks
            </>
          )}
        </button>
      </div>

      {pickerOpen && (
        <HookTemplatePicker
          onInsert={insertTemplate}
          onClose={() => setPickerOpen(false)}
        />
      )}
    </div>
  );
}

// ── Per-event section ────────────────────────────────────────────────────────

interface HookSectionProps {
  event: HookEvent;
  entries: HookEntry[];
  onChange: (next: HookEntry[]) => void;
}

function HookSection({ event, entries, onChange }: HookSectionProps) {
  const { title, subtitle } = EVENT_LABELS[event];
  const showMatcher = event !== 'stop';

  const updateEntry = (idx: number, patch: Partial<HookEntry>) => {
    const next = entries.map((e, i) => (i === idx ? { ...e, ...patch } : e));
    onChange(next);
  };
  const removeEntry = (idx: number) => {
    onChange(entries.filter((_, i) => i !== idx));
  };
  const addEntry = () => {
    onChange([...entries, emptyEntry()]);
  };

  return (
    <section className="rounded-xl border border-edge-primary/30 bg-surface-secondary/40 p-4">
      <header className="mb-3 flex items-baseline justify-between gap-3">
        <div>
          <h3 className="text-[13px] font-semibold text-content-primary">{title}</h3>
          <p className="mt-0.5 text-[11px] text-content-tertiary">{subtitle}</p>
        </div>
        <span className="rounded-full bg-surface-tertiary/60 px-2 py-0.5 text-[10px] font-medium tabular-nums text-content-tertiary">
          {entries.length}
        </span>
      </header>

      <div className="flex flex-col gap-2">
        {entries.length === 0 && (
          <p className="rounded-lg border border-dashed border-edge-primary/40 bg-surface-input/30 px-3 py-3 text-center text-[11px] text-content-tertiary">
            No {title} hooks configured.
          </p>
        )}
        {entries.map((entry, idx) => (
          <div
            key={idx}
            className="rounded-lg border border-edge-primary/30 bg-surface-input/40 p-3"
          >
            <div className="flex items-center justify-between gap-2 pb-2">
              <span className="text-[10.5px] font-medium uppercase tracking-wider text-content-tertiary">
                #{idx + 1}
              </span>
              <button
                onClick={() => removeEntry(idx)}
                className="flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] text-content-tertiary
                           transition-colors hover:bg-rose-500/10 hover:text-rose-600 dark:hover:text-rose-400"
                title="Delete hook"
              >
                <Trash2 className="h-3 w-3" />
                Delete
              </button>
            </div>
            <div className="grid grid-cols-1 gap-2 sm:grid-cols-[1fr_120px]">
              {showMatcher && (
                <label className="flex flex-col gap-1">
                  <span className="text-[10.5px] font-medium text-content-tertiary">
                    Matcher
                  </span>
                  <input
                    value={entry.matcher}
                    onChange={(e) => updateEntry(idx, { matcher: e.target.value })}
                    placeholder='Tool name (e.g. "Bash") or "*"'
                    className="w-full rounded-md border border-edge-primary/40 bg-surface-input/60 px-2.5 py-1.5
                               text-[12px] text-content-primary outline-none placeholder:text-content-tertiary
                               focus:border-themed-accent/50 focus:ring-1 focus:ring-themed-accent/20"
                  />
                </label>
              )}
              <label className={`flex flex-col gap-1 ${showMatcher ? '' : 'sm:col-span-1'}`}>
                <span className="text-[10.5px] font-medium text-content-tertiary">
                  Timeout (s)
                </span>
                <input
                  type="number"
                  min={1}
                  max={300}
                  value={entry.timeout_secs ?? ''}
                  onChange={(e) => {
                    const raw = e.target.value;
                    const next =
                      raw === '' ? null : Math.max(1, Math.min(300, Number(raw) || 1));
                    updateEntry(idx, { timeout_secs: next });
                  }}
                  placeholder="30"
                  className="w-full rounded-md border border-edge-primary/40 bg-surface-input/60 px-2.5 py-1.5
                             text-[12px] text-content-primary outline-none placeholder:text-content-tertiary
                             focus:border-themed-accent/50 focus:ring-1 focus:ring-themed-accent/20"
                />
              </label>
            </div>
            <label className="mt-2 flex flex-col gap-1">
              <span className="text-[10.5px] font-medium text-content-tertiary">Command</span>
              <textarea
                value={entry.command}
                onChange={(e) => updateEntry(idx, { command: e.target.value })}
                rows={2}
                placeholder='echo "fired"; exit 0'
                spellCheck={false}
                className="w-full rounded-md border border-edge-primary/40 bg-surface-input/60 px-2.5 py-1.5
                           font-mono text-[11.5px] text-content-primary outline-none placeholder:text-content-tertiary
                           focus:border-themed-accent/50 focus:ring-1 focus:ring-themed-accent/20"
              />
            </label>
          </div>
        ))}

        <button
          onClick={addEntry}
          className="flex items-center justify-center gap-1.5 rounded-lg border border-dashed border-edge-primary/50
                     bg-surface-input/20 px-3 py-2 text-[11.5px] font-medium text-content-secondary
                     transition-colors hover:border-themed-accent/50 hover:bg-surface-tertiary/40 hover:text-content-primary"
        >
          <Plus className="h-3.5 w-3.5" />
          Add {title} hook
        </button>
      </div>
    </section>
  );
}
