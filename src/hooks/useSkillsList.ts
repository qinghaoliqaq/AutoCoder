/**
 * useSkillsList — calls the `list_skills` Tauri command and exposes the
 * deduplicated skill registry to the UI.
 *
 * Refreshes whenever the workspace changes (project-scoped SKILL.md
 * files come and go with the active workspace) and supports a manual
 * `refresh()` for cases where the user just edited a SKILL.md and wants
 * to see the new version without restarting the app.
 *
 * The pure helpers `filterSkills` / `groupByProvider` live alongside the
 * hook so unit tests can exercise the panel's filter + group logic
 * without rendering React.
 */

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { SkillSummary, SkillProvider } from '../types';

export interface UseSkillsListResult {
  skills: SkillSummary[];
  loading: boolean;
  error: string | null;
  refresh: () => void;
}

export function useSkillsList(workspace: string | null): UseSkillsListResult {
  const [skills, setSkills] = useState<SkillSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    invoke<SkillSummary[]>('list_skills', { workspace })
      .then((list) => {
        if (cancelled) return;
        setSkills(list);
      })
      .catch((err) => {
        if (cancelled) return;
        setError(String(err));
      })
      .finally(() => {
        if (cancelled) return;
        setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [workspace, version]);

  const refresh = useCallback(() => setVersion((v) => v + 1), []);

  return { skills, loading, error, refresh };
}

// ── Pure helpers (testable without React) ────────────────────────────────────

/**
 * Case-insensitive substring filter against name + label + description.
 * An empty / whitespace-only query returns the input unchanged.
 */
export function filterSkills(skills: SkillSummary[], query: string): SkillSummary[] {
  const q = query.trim().toLowerCase();
  if (!q) return skills;
  return skills.filter((s) => {
    const haystack = `${s.name} ${s.label} ${s.description}`.toLowerCase();
    return haystack.includes(q);
  });
}

const PROVIDER_ORDER: SkillProvider[] = ['project', 'user', 'claude', 'codex', 'builtin'];

/**
 * Group skills by provider in a stable display order: project-scoped
 * overrides first, then user, then upstream skill ecosystems, then
 * builtins. Empty groups are dropped.
 */
export function groupByProvider(
  skills: SkillSummary[],
): Array<{ provider: SkillProvider; items: SkillSummary[] }> {
  const buckets = new Map<SkillProvider, SkillSummary[]>();
  for (const s of skills) {
    const list = buckets.get(s.provider);
    if (list) {
      list.push(s);
    } else {
      buckets.set(s.provider, [s]);
    }
  }
  return PROVIDER_ORDER.flatMap((p) => {
    const items = buckets.get(p);
    if (!items || items.length === 0) return [];
    // Sort within group by name so the list is deterministic across reloads.
    const sorted = [...items].sort((a, b) => a.name.localeCompare(b.name));
    return [{ provider: p, items: sorted }];
  });
}

export const PROVIDER_LABELS: Record<SkillProvider, string> = {
  project: 'Project',
  user: 'User',
  claude: 'Claude Code',
  codex: 'Codex',
  builtin: 'Built-in',
};
