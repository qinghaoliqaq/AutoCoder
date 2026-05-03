import { describe, it, expect } from 'vitest';
import { HOOK_TEMPLATES, findTemplate } from '../hookTemplates';
import { validateEntry } from '../components/HooksTab';

const VALID_EVENTS = new Set(['pre_tool_use', 'post_tool_use', 'stop']);

describe('HOOK_TEMPLATES integrity', () => {
  it('has at least one template', () => {
    expect(HOOK_TEMPLATES.length).toBeGreaterThan(0);
  });

  it('every template has a unique id', () => {
    const ids = HOOK_TEMPLATES.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('every template has the required string fields populated', () => {
    for (const t of HOOK_TEMPLATES) {
      expect(t.id).toMatch(/^[a-z0-9-]+$/); // kebab-case
      expect(t.name.trim()).not.toBe('');
      expect(t.description.trim()).not.toBe('');
      expect(t.entry.command.trim()).not.toBe('');
    }
  });

  it('every template targets a valid HookEvent', () => {
    for (const t of HOOK_TEMPLATES) {
      expect(VALID_EVENTS.has(t.event)).toBe(true);
    }
  });

  it('every template entry passes validateEntry for its declared event', () => {
    // Catches drift between what the picker would insert and what the
    // editor accepts — if a template can't pass validation, "insert"
    // would put the form in a permanently-invalid state.
    for (const t of HOOK_TEMPLATES) {
      const err = validateEntry(t.entry, t.event);
      expect(err, `template ${t.id} failed validation: ${err ?? ''}`).toBeNull();
    }
  });

  it('covers all three events (a non-empty template per event)', () => {
    // Sanity: the picker shows three sections; each should have at
    // least one starter so users can discover patterns at every
    // lifecycle point.
    for (const event of ['pre_tool_use', 'post_tool_use', 'stop'] as const) {
      const matches = HOOK_TEMPLATES.filter((t) => t.event === event);
      expect(matches.length).toBeGreaterThan(0);
    }
  });
});

describe('findTemplate', () => {
  it('returns the template by id', () => {
    const t = HOOK_TEMPLATES[0];
    expect(findTemplate(t.id)).toBe(t);
  });

  it('returns undefined for an unknown id', () => {
    expect(findTemplate('does-not-exist')).toBeUndefined();
  });
});
