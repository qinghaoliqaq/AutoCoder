/**
 * HookTemplatePicker — modal-style overlay listing curated hook
 * templates from `hookTemplates.ts`. Click a template → it inserts as a
 * new entry into the parent's `HooksConfig` for the matching event.
 *
 * Lives inside HooksTab; doesn't manage its own persistence.
 */

import { useMemo, useState } from 'react';
import { ChevronRight, Search, Sparkles, X } from 'lucide-react';
import { HOOK_TEMPLATES, type HookTemplate } from '../hookTemplates';
import type { HookEvent } from '../types';

interface Props {
  onInsert: (template: HookTemplate) => void;
  onClose: () => void;
}

const EVENT_LABEL: Record<HookEvent, string> = {
  pre_tool_use: 'PreToolUse',
  post_tool_use: 'PostToolUse',
  stop: 'Stop',
};

const EVENT_ORDER: HookEvent[] = ['pre_tool_use', 'post_tool_use', 'stop'];

export default function HookTemplatePicker({ onInsert, onClose }: Props) {
  const [query, setQuery] = useState('');

  const grouped = useMemo(() => {
    const q = query.trim().toLowerCase();
    const matches = (t: HookTemplate) =>
      !q ||
      `${t.name} ${t.description} ${t.entry.command} ${t.entry.matcher}`
        .toLowerCase()
        .includes(q);
    return EVENT_ORDER.flatMap((event) => {
      const items = HOOK_TEMPLATES.filter((t) => t.event === event && matches(t));
      return items.length ? [{ event, items }] : [];
    });
  }, [query]);

  return (
    <div
      className="fixed inset-0 z-[60] flex items-center justify-center bg-white/20 backdrop-blur-xl dark:bg-zinc-950/60"
      onClick={onClose}
      onKeyDown={(e) => {
        if (e.key === 'Escape') onClose();
      }}
    >
      <div
        className="mx-4 flex max-h-[80vh] w-full max-w-2xl flex-col overflow-hidden glass-panel"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-start justify-between gap-3 border-b border-zinc-200/40 bg-white/10 px-5 py-4 dark:border-zinc-800/50 dark:bg-zinc-900/25">
          <div className="flex items-start gap-3 min-w-0 flex-1">
            <div className="mt-0.5 rounded-full bg-amber-100/70 p-1.5 text-amber-700 dark:bg-amber-900/30 dark:text-amber-300">
              <Sparkles className="h-4 w-4" />
            </div>
            <div className="min-w-0 flex-1">
              <h2 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200">
                Hook templates
              </h2>
              <p className="text-xs text-zinc-500 mt-0.5">
                Click a template to insert it. You can edit matcher / command / timeout afterwards.
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="rounded-lg p-1 text-zinc-400 transition-colors hover:bg-zinc-100 hover:text-zinc-600 dark:hover:bg-zinc-800 dark:hover:text-zinc-300"
            title="Close (Esc)"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* Search */}
        <div className="px-4 py-2.5 border-b border-zinc-200/30 dark:border-zinc-800/40">
          <label className="relative flex items-center">
            <Search className="absolute left-2.5 h-3.5 w-3.5 text-zinc-400 pointer-events-none" />
            <input
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Filter templates…"
              className="w-full rounded-md border border-edge-primary/40 bg-surface-input/40 pl-8 pr-2 py-1.5 text-xs
                         text-content-primary placeholder:text-content-tertiary outline-none
                         focus:border-themed-accent/50 focus:ring-1 focus:ring-themed-accent/20"
            />
          </label>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto custom-scrollbar p-3">
          {grouped.length === 0 ? (
            <p className="px-2 py-6 text-center text-xs text-zinc-500">
              No templates match your filter.
            </p>
          ) : (
            grouped.map(({ event, items }) => (
              <section key={event} className="mb-4 last:mb-0">
                <h3 className="px-2 pb-1.5 text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
                  {EVENT_LABEL[event]}
                  <span className="ml-1.5 font-normal opacity-70">{items.length}</span>
                </h3>
                <ul className="flex flex-col gap-1.5">
                  {items.map((tpl) => (
                    <li key={tpl.id}>
                      <button
                        onClick={() => {
                          onInsert(tpl);
                          onClose();
                        }}
                        className="group flex w-full flex-col items-start gap-1 rounded-lg border border-edge-primary/30
                                   bg-surface-input/30 p-3 text-left transition-colors hover:border-themed-accent/40
                                   hover:bg-surface-tertiary/40"
                      >
                        <div className="flex w-full items-center justify-between gap-2">
                          <span className="text-xs font-semibold text-content-primary group-hover:text-themed-accent-text">
                            {tpl.name}
                          </span>
                          <ChevronRight className="h-3.5 w-3.5 text-content-tertiary transition-transform group-hover:translate-x-0.5 group-hover:text-themed-accent" />
                        </div>
                        <p className="text-[11px] leading-relaxed text-content-secondary">
                          {tpl.description}
                        </p>
                        {tpl.platform_note && (
                          <p className="text-[10.5px] italic text-content-tertiary">
                            {tpl.platform_note}
                          </p>
                        )}
                        <div className="mt-1 flex flex-wrap items-center gap-1.5 text-[10px] text-content-tertiary">
                          <span className="rounded bg-surface-tertiary/60 px-1.5 py-0.5 font-mono">
                            matcher: {tpl.entry.matcher}
                          </span>
                          <span className="rounded bg-surface-tertiary/60 px-1.5 py-0.5 font-mono">
                            timeout: {tpl.entry.timeout_secs ?? 'default'}s
                          </span>
                        </div>
                      </button>
                    </li>
                  ))}
                </ul>
              </section>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
