import { describe, it, expect } from 'vitest';
import { parseAttr, parseInvoke, stripInvoke } from '../invoke';

// ── parseAttr ─────────────────────────────────────────────────────────────────

describe('parseAttr', () => {
  it('returns simple value', () => {
    expect(parseAttr('skill="plan"', 'skill')).toBe('plan');
  });

  it('returns undefined when attribute is absent', () => {
    expect(parseAttr('skill="plan"', 'dir')).toBeUndefined();
  });

  it('unescapes \\" inside value', () => {
    expect(parseAttr('task="say \\"hello\\""', 'task')).toBe('say "hello"');
  });

  it('unescapes \\\\ inside value', () => {
    expect(parseAttr('task="C:\\\\Users\\\\foo"', 'task')).toBe('C:\\Users\\foo');
  });

  it('handles value containing single quotes', () => {
    expect(parseAttr("task=\"it's fine\"", 'task')).toBe("it's fine");
  });
});

// ── parseInvoke ───────────────────────────────────────────────────────────────

describe('parseInvoke', () => {
  it('parses a minimal valid invoke tag', () => {
    const result = parseInvoke('<invoke skill="plan" task="Design the DB schema" />');
    expect(result).toEqual({ skill: 'plan', task: 'Design the DB schema', dir: undefined });
  });

  it('parses all three attributes', () => {
    const result = parseInvoke('<invoke skill="code" task="Add auth" dir="/workspace/app" />');
    expect(result).toEqual({ skill: 'code', task: 'Add auth', dir: '/workspace/app' });
  });

  it('returns null when skill is unknown', () => {
    expect(parseInvoke('<invoke skill="fly" task="go" />')).toBeNull();
  });

  it('returns null when tag is absent', () => {
    expect(parseInvoke('Just a normal reply.')).toBeNull();
  });

  it('handles task with double-quote escape', () => {
    const result = parseInvoke('<invoke skill="debug" task="Fix the \\"NaN\\" error" />');
    expect(result?.task).toBe('Fix the "NaN" error');
  });

  it('handles task with newlines (multiline attr body)', () => {
    const result = parseInvoke('<invoke skill="test" task="line1\nline2" />');
    expect(result?.task).toBe('line1\nline2');
  });

  it('accepts all valid skill values', () => {
    for (const skill of ['plan', 'code', 'debug', 'test', 'review'] as const) {
      expect(parseInvoke(`<invoke skill="${skill}" task="x" />`)).not.toBeNull();
    }
  });

  it('tolerates extra whitespace around attributes', () => {
    const result = parseInvoke('<invoke  skill="review"  task="check it"  />');
    expect(result?.skill).toBe('review');
  });
});

// ── stripInvoke ───────────────────────────────────────────────────────────────

describe('stripInvoke', () => {
  it('removes trailing invoke tag', () => {
    const text = 'Sure, I will plan.\n<invoke skill="plan" task="Design DB" />';
    expect(stripInvoke(text)).toBe('Sure, I will plan.');
  });

  it('leaves text unchanged when no invoke tag present', () => {
    const text = 'Just a normal reply.';
    expect(stripInvoke(text)).toBe(text);
  });

  it('removes trailing whitespace after stripping', () => {
    const text = 'Prefix   \n<invoke skill="code" task="x" />   ';
    expect(stripInvoke(text)).toBe('Prefix');
  });
});
