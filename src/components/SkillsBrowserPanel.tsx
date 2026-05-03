/**
 * SkillsBrowserPanel — sidebar panel that lists every skill the agent
 * can invoke, grouped by where it came from. Click a skill to open an
 * inline preview with the full markdown body.
 *
 * Backed by the `list_skills` Tauri command (which calls
 * `SkillRegistry::discover` on the workspace, deduplicating across
 * project / user / Claude / Codex / builtin sources). The panel is
 * read-only — it surfaces the chain graph to humans; the model still
 * invokes skills through the existing `Skill` tool.
 */

import { useMemo, useState } from 'react';
import ReactMarkdown from 'react-markdown';
import rehypeSanitize from 'rehype-sanitize';
import remarkGfm from 'remark-gfm';
import { ChevronLeft, RefreshCw, Search, Sparkles, X } from 'lucide-react';
import type { SkillSummary } from '../types';
import {
  PROVIDER_LABELS,
  filterSkills,
  groupByProvider,
} from '../hooks/useSkillsList';

interface Props {
  skills: SkillSummary[];
  loading: boolean;
  error: string | null;
  onRefresh: () => void;
  onClose: () => void;
}

export default function SkillsBrowserPanel({
  skills,
  loading,
  error,
  onRefresh,
  onClose,
}: Props) {
  const [query, setQuery] = useState('');
  const [activeName, setActiveName] = useState<string | null>(null);

  const filtered = useMemo(() => filterSkills(skills, query), [skills, query]);
  const grouped = useMemo(() => groupByProvider(filtered), [filtered]);

  const active = activeName
    ? skills.find((s) => s.name === activeName) ?? null
    : null;

  return (
    <div className="flex flex-col h-full w-full overflow-hidden">
      {/* Header */}
      <div
        className="flex items-center justify-between gap-2 px-5 py-3 border-b
                   border-edge-primary/40 flex-shrink-0 min-h-[48px]"
        style={{ backgroundColor: 'rgb(var(--bg-secondary) / 0.2)' }}
      >
        <div className="flex items-center gap-2.5 min-w-0">
          <span className="text-[11px] font-bold uppercase tracking-widest text-content-primary select-none">
            Skills
          </span>
          {skills.length > 0 && (
            <span className="rounded-full px-1.5 py-0.5 text-[10px] font-semibold tabular-nums bg-surface-tertiary/80 text-content-secondary">
              {skills.length}
            </span>
          )}
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={onRefresh}
            disabled={loading}
            className="flex h-6 w-6 items-center justify-center rounded-md text-content-tertiary
                       transition-colors hover:bg-surface-tertiary/50 hover:text-content-primary
                       disabled:cursor-not-allowed disabled:opacity-40"
            title="Refresh"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${loading ? 'animate-spin' : ''}`} />
          </button>
          <button
            onClick={onClose}
            className="flex h-6 w-6 items-center justify-center rounded-md text-content-tertiary
                       transition-colors hover:bg-surface-tertiary/50 hover:text-content-primary"
            title="Close"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {/* Search */}
      <div className="px-3 py-2 border-b border-edge-primary/30 flex-shrink-0">
        <label className="relative flex items-center">
          <Search className="absolute left-2.5 h-3.5 w-3.5 text-content-tertiary pointer-events-none" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Filter skills…"
            className="w-full rounded-md border border-edge-primary/40 bg-surface-input/40 pl-8 pr-2 py-1.5 text-xs
                       text-content-primary placeholder:text-content-tertiary outline-none
                       focus:border-themed-accent/50 focus:ring-1 focus:ring-themed-accent/20"
          />
        </label>
      </div>

      {/* Body */}
      <div className="flex-1 overflow-y-auto custom-scrollbar relative">
        {error ? (
          <div className="p-4 text-xs text-rose-500">Failed to load skills: {error}</div>
        ) : loading && skills.length === 0 ? (
          <div className="p-4 text-xs text-content-tertiary">Loading skills…</div>
        ) : grouped.length === 0 ? (
          <div className="flex flex-col items-center justify-center gap-2 p-8 text-content-tertiary">
            <Sparkles className="h-6 w-6 opacity-40" />
            <p className="text-xs">
              {query.trim() ? 'No skills match your filter.' : 'No skills available.'}
            </p>
          </div>
        ) : (
          <ul className="px-2 py-2">
            {grouped.map((group) => (
              <li key={group.provider} className="mb-3 last:mb-0">
                <div className="px-2 pb-1 text-[10px] font-semibold uppercase tracking-wider text-content-tertiary/80">
                  {PROVIDER_LABELS[group.provider]}
                  <span className="ml-1.5 font-normal opacity-70">
                    {group.items.length}
                  </span>
                </div>
                <ul>
                  {group.items.map((s) => (
                    <li key={`${group.provider}:${s.name}`}>
                      <button
                        onClick={() => setActiveName(s.name)}
                        className="group flex w-full flex-col items-start gap-0.5 rounded-md px-2 py-1.5
                                   text-left text-xs transition-colors
                                   hover:bg-surface-tertiary/50"
                      >
                        <div className="flex w-full items-baseline justify-between gap-2">
                          <code className="font-mono text-[11.5px] text-content-primary group-hover:text-themed-accent-text">
                            /{s.name}
                          </code>
                          {s.related.length > 0 && (
                            <span
                              className="text-[10px] tabular-nums text-content-tertiary"
                              title={`${s.related.length} related skill${s.related.length === 1 ? '' : 's'}`}
                            >
                              ⇢{s.related.length}
                            </span>
                          )}
                        </div>
                        <p className="line-clamp-2 text-[11px] text-content-secondary">
                          {s.description}
                        </p>
                      </button>
                    </li>
                  ))}
                </ul>
              </li>
            ))}
          </ul>
        )}

        {/* Preview drawer */}
        {active && (
          <div className="absolute inset-0 z-10 flex flex-col bg-surface-primary/95 backdrop-blur-sm">
            <div className="flex items-center justify-between gap-2 border-b border-edge-primary/40 px-4 py-2.5">
              <button
                onClick={() => setActiveName(null)}
                className="flex items-center gap-1 text-[11px] font-medium text-content-secondary
                           transition-colors hover:text-content-primary"
              >
                <ChevronLeft className="h-3.5 w-3.5" />
                Back
              </button>
              <span className="rounded-full px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wider
                             bg-surface-tertiary/80 text-content-tertiary">
                {PROVIDER_LABELS[active.provider]}
              </span>
            </div>
            <div className="overflow-y-auto custom-scrollbar p-4">
              <h2 className="mb-1 text-sm font-semibold text-content-primary">
                {active.label}
              </h2>
              <code className="text-[11px] text-content-tertiary">/{active.name}</code>
              <p className="mt-2 text-xs italic text-content-secondary">
                {active.description}
              </p>
              {active.related.length > 0 && (
                <div className="mt-3 flex flex-wrap gap-1.5">
                  {active.related.map((rel) => (
                    <button
                      key={rel}
                      onClick={() => setActiveName(rel)}
                      className="rounded-md bg-surface-tertiary/60 px-2 py-0.5 text-[10.5px] font-medium
                                 text-content-secondary transition-colors hover:bg-surface-tertiary
                                 hover:text-content-primary"
                    >
                      ↪ /{rel}
                    </button>
                  ))}
                </div>
              )}
              <hr className="my-3 border-edge-primary/30" />
              <article className="prose prose-sm prose-zinc dark:prose-invert max-w-none text-[12.5px]">
                <ReactMarkdown
                  remarkPlugins={[remarkGfm]}
                  rehypePlugins={[rehypeSanitize]}
                >
                  {active.content}
                </ReactMarkdown>
              </article>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
