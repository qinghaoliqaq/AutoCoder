import { describe, it, expect } from 'vitest';
import {
  filterSkills,
  groupByProvider,
  PROVIDER_LABELS,
} from '../hooks/useSkillsList';
import type { SkillSummary } from '../types';

function makeSkill(overrides: Partial<SkillSummary>): SkillSummary {
  return {
    name: 'verify',
    label: 'Verify',
    description: 'Verify code changes work end-to-end.',
    category: 'testing',
    content: '# body',
    provider: 'builtin',
    related: [],
    ...overrides,
  };
}

describe('filterSkills', () => {
  const skills: SkillSummary[] = [
    makeSkill({ name: 'simplify', label: 'Simplify', description: 'Code review.' }),
    makeSkill({ name: 'verify', label: 'Verify', description: 'End-to-end check.' }),
    makeSkill({ name: 'frontend-dev', label: 'Frontend Dev', description: 'UI work.' }),
  ];

  it('returns all skills for an empty query', () => {
    expect(filterSkills(skills, '')).toEqual(skills);
    expect(filterSkills(skills, '   ')).toEqual(skills);
  });

  it('filters by name (case-insensitive)', () => {
    const out = filterSkills(skills, 'SIMPLI');
    expect(out.map((s) => s.name)).toEqual(['simplify']);
  });

  it('filters by label', () => {
    const out = filterSkills(skills, 'frontend');
    expect(out.map((s) => s.name)).toEqual(['frontend-dev']);
  });

  it('filters by description text', () => {
    const out = filterSkills(skills, 'end-to-end');
    expect(out.map((s) => s.name)).toEqual(['verify']);
  });

  it('returns empty when nothing matches', () => {
    expect(filterSkills(skills, 'no-such-thing')).toEqual([]);
  });
});

describe('groupByProvider', () => {
  it('groups skills by provider in priority order', () => {
    const skills: SkillSummary[] = [
      makeSkill({ name: 'a', provider: 'builtin' }),
      makeSkill({ name: 'b', provider: 'project' }),
      makeSkill({ name: 'c', provider: 'claude' }),
      makeSkill({ name: 'd', provider: 'builtin' }),
      makeSkill({ name: 'e', provider: 'codex' }),
    ];

    const grouped = groupByProvider(skills);
    expect(grouped.map((g) => g.provider)).toEqual([
      'project',
      'claude',
      'codex',
      'builtin',
    ]);
    // Within each group, items sort alphabetically by name.
    expect(grouped.find((g) => g.provider === 'builtin')!.items.map((s) => s.name))
      .toEqual(['a', 'd']);
  });

  it('drops empty groups', () => {
    const skills = [makeSkill({ provider: 'builtin' })];
    const grouped = groupByProvider(skills);
    expect(grouped).toHaveLength(1);
    expect(grouped[0].provider).toBe('builtin');
  });

  it('handles user-scoped skills before claude/codex', () => {
    const skills: SkillSummary[] = [
      makeSkill({ name: 'a', provider: 'claude' }),
      makeSkill({ name: 'b', provider: 'user' }),
      makeSkill({ name: 'c', provider: 'codex' }),
    ];
    const grouped = groupByProvider(skills);
    expect(grouped.map((g) => g.provider)).toEqual(['user', 'claude', 'codex']);
  });

  it('returns an empty list when input is empty', () => {
    expect(groupByProvider([])).toEqual([]);
  });
});

describe('PROVIDER_LABELS', () => {
  it('has a label for every provider variant', () => {
    // Defensive — adding a new SkillProvider variant must also add a label
    // here, otherwise the panel renders `undefined` as a section header.
    const expected = ['builtin', 'project', 'user', 'claude', 'codex'] as const;
    for (const p of expected) {
      expect(PROVIDER_LABELS[p]).toBeTruthy();
    }
  });
});
