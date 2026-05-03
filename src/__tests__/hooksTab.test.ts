import { describe, it, expect } from 'vitest';
import {
  emptyEntry,
  validateEntry,
  cleanForSave,
} from '../components/HooksTab';
import type { HookEntry, HooksConfig } from '../types';

describe('emptyEntry', () => {
  it('produces a wildcard matcher with no command', () => {
    const e = emptyEntry();
    expect(e.matcher).toBe('*');
    expect(e.command).toBe('');
    expect(e.timeout_secs).toBeNull();
  });
});

describe('validateEntry', () => {
  const ok: HookEntry = { matcher: 'Bash', command: 'true', timeout_secs: null };

  it('accepts a well-formed entry', () => {
    expect(validateEntry(ok, 'pre_tool_use')).toBeNull();
    expect(validateEntry(ok, 'post_tool_use')).toBeNull();
  });

  it('rejects empty command', () => {
    expect(validateEntry({ ...ok, command: '' }, 'pre_tool_use')).toContain('Command');
    expect(validateEntry({ ...ok, command: '   ' }, 'pre_tool_use')).toContain('Command');
  });

  it('rejects empty matcher for tool-use events', () => {
    expect(validateEntry({ ...ok, matcher: '' }, 'pre_tool_use')).toContain('Matcher');
    expect(validateEntry({ ...ok, matcher: '' }, 'post_tool_use')).toContain('Matcher');
  });

  it('does NOT require matcher for stop hooks', () => {
    expect(validateEntry({ ...ok, matcher: '' }, 'stop')).toBeNull();
  });

  it('rejects out-of-range timeouts', () => {
    expect(validateEntry({ ...ok, timeout_secs: 0 }, 'pre_tool_use')).toContain('Timeout');
    expect(validateEntry({ ...ok, timeout_secs: 301 }, 'pre_tool_use')).toContain('Timeout');
  });

  it('accepts boundary timeouts', () => {
    expect(validateEntry({ ...ok, timeout_secs: 1 }, 'pre_tool_use')).toBeNull();
    expect(validateEntry({ ...ok, timeout_secs: 300 }, 'pre_tool_use')).toBeNull();
  });

  it('accepts null timeout (default)', () => {
    expect(validateEntry({ ...ok, timeout_secs: null }, 'pre_tool_use')).toBeNull();
  });
});

describe('cleanForSave', () => {
  it('trims matcher and command on tool-use entries', () => {
    const raw: HooksConfig = {
      pre_tool_use: [{ matcher: '  Bash  ', command: '  true  ', timeout_secs: 5 }],
      post_tool_use: [],
      stop: [],
    };
    const out = cleanForSave(raw);
    expect(out.pre_tool_use[0]).toEqual({
      matcher: 'Bash',
      command: 'true',
      timeout_secs: 5,
    });
  });

  it('forces stop hook matcher to "*" regardless of input', () => {
    // The backend ignores matcher for stop hooks but a stale value
    // would surface confusingly on next load — normalize to "*".
    const raw: HooksConfig = {
      pre_tool_use: [],
      post_tool_use: [],
      stop: [{ matcher: 'leftover-from-previous-event', command: 'echo done', timeout_secs: null }],
    };
    const out = cleanForSave(raw);
    expect(out.stop[0].matcher).toBe('*');
    expect(out.stop[0].command).toBe('echo done');
  });

  it('falls back matcher to "*" if user trimmed it to empty', () => {
    const raw: HooksConfig = {
      pre_tool_use: [{ matcher: '   ', command: 'true', timeout_secs: null }],
      post_tool_use: [],
      stop: [],
    };
    const out = cleanForSave(raw);
    // The matcher field is required by the backend, so we never send
    // an empty string — `*` is the safe fallback.
    expect(out.pre_tool_use[0].matcher).toBe('*');
  });

  it('preserves the structural shape across all event arrays', () => {
    const raw: HooksConfig = {
      pre_tool_use: [{ matcher: 'A', command: 'a', timeout_secs: null }],
      post_tool_use: [{ matcher: 'B', command: 'b', timeout_secs: 10 }],
      stop: [{ matcher: 'C', command: 'c', timeout_secs: null }],
    };
    const out = cleanForSave(raw);
    expect(out.pre_tool_use).toHaveLength(1);
    expect(out.post_tool_use).toHaveLength(1);
    expect(out.stop).toHaveLength(1);
    // Stop matcher normalized; others kept.
    expect(out.pre_tool_use[0].matcher).toBe('A');
    expect(out.post_tool_use[0].matcher).toBe('B');
    expect(out.stop[0].matcher).toBe('*');
  });
});
