import { describe, it, expect } from 'vitest';
import { MODES, type AppMode } from '../types';

const VALID_SKILLS: AppMode[] = ['plan', 'code', 'debug', 'test', 'review'];
const VALID_LEADERS = ['claude', 'codex', 'director', 'user'];

describe('MODES', () => {
  it('contains exactly the 5 expected skill modes', () => {
    const ids = MODES.map(m => m.id);
    expect(ids.sort()).toEqual([...VALID_SKILLS].sort());
  });

  it('has no duplicate ids', () => {
    const ids = MODES.map(m => m.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('every mode has all required fields non-empty', () => {
    for (const m of MODES) {
      expect(m.id,          `${m.id}: id`         ).toBeTruthy();
      expect(m.label,       `${m.id}: label`       ).toBeTruthy();
      expect(m.icon,        `${m.id}: icon`        ).toBeTruthy();
      expect(m.description, `${m.id}: description` ).toBeTruthy();
      expect(m.color,       `${m.id}: color`       ).toBeTruthy();
    }
  });

  it('every mode has a valid leader role', () => {
    for (const m of MODES) {
      expect(VALID_LEADERS, `${m.id}: leader`).toContain(m.leader);
    }
  });

  it('plan mode requiresBoth (needs claude + codex)', () => {
    const plan = MODES.find(m => m.id === 'plan');
    expect(plan?.requiresBoth).toBe(true);
  });

  it('non-plan modes do not requireBoth', () => {
    for (const m of MODES.filter(m => m.id !== 'plan')) {
      expect(m.requiresBoth, `${m.id} should not requiresBoth`).toBe(false);
    }
  });
});
