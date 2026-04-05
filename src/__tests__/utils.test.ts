import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { relativeTime, getGroup, makeId, makeSessionId, syncSessionIdentity } from '../utils';

// ── relativeTime ──────────────────────────────────────────────────────────────

describe('relativeTime', () => {
  let now: number;

  beforeEach(() => {
    now = Date.now();
    vi.useFakeTimers();
    vi.setSystemTime(now);
  });

  afterEach(() => { vi.useRealTimers(); });

  it('returns 刚刚 for < 1 minute ago', () => {
    expect(relativeTime(now - 30_000)).toBe('刚刚');
  });

  it('returns X分钟前 for 1–59 minutes ago', () => {
    expect(relativeTime(now - 5 * 60_000)).toBe('5分钟前');
    expect(relativeTime(now - 59 * 60_000)).toBe('59分钟前');
  });

  it('returns X小时前 for 1–23 hours ago', () => {
    expect(relativeTime(now - 1 * 3_600_000)).toBe('1小时前');
    expect(relativeTime(now - 23 * 3_600_000)).toBe('23小时前');
  });

  it('returns X天前 for 1–6 days ago', () => {
    expect(relativeTime(now - 1 * 86_400_000)).toBe('1天前');
    expect(relativeTime(now - 6 * 86_400_000)).toBe('6天前');
  });

  it('returns locale date string for >= 7 days ago', () => {
    const ts = now - 7 * 86_400_000;
    const expected = new Date(ts).toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' });
    expect(relativeTime(ts)).toBe(expected);
  });
});

// ── getGroup ──────────────────────────────────────────────────────────────────

describe('getGroup', () => {
  // Pin "now" to 2024-06-15 12:00:00 UTC for deterministic day calculations
  const BASE = new Date('2024-06-15T12:00:00Z').getTime();

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(BASE);
  });

  afterEach(() => { vi.useRealTimers(); });

  const daysAgo = (d: number) => BASE - d * 86_400_000;

  it('returns 今天 for today', () => {
    expect(getGroup(daysAgo(0))).toBe('今天');
  });

  it('returns 昨天 for yesterday', () => {
    expect(getGroup(daysAgo(1))).toBe('昨天');
  });

  it('returns 最近 7 天 for 2–7 days ago', () => {
    expect(getGroup(daysAgo(2))).toBe('最近 7 天');
    expect(getGroup(daysAgo(7))).toBe('最近 7 天');
  });

  it('returns 最近 30 天 for 8–30 days ago', () => {
    expect(getGroup(daysAgo(8))).toBe('最近 30 天');
    expect(getGroup(daysAgo(30))).toBe('最近 30 天');
  });

  it('returns month/year locale string for > 30 days ago', () => {
    const ts = daysAgo(31);
    const expected = new Date(ts).toLocaleDateString('zh-CN', { year: 'numeric', month: 'long' });
    expect(getGroup(ts)).toBe(expected);
  });
});

// ── makeId ────────────────────────────────────────────────────────────────────

describe('makeId', () => {
  it('returns string with msg- prefix', () => {
    expect(makeId()).toMatch(/^msg-/);
  });

  it('returns unique values on successive calls', () => {
    const ids = new Set(Array.from({ length: 20 }, () => makeId()));
    expect(ids.size).toBe(20);
  });
});

// ── makeSessionId ─────────────────────────────────────────────────────────────

describe('makeSessionId', () => {
  it('returns string with sess- prefix', () => {
    expect(makeSessionId()).toMatch(/^sess-/);
  });

  it('returns unique values on successive calls', () => {
    const ids = new Set(Array.from({ length: 20 }, () => makeSessionId()));
    expect(ids.size).toBe(20);
  });
});

// ── syncSessionIdentity ────────────────────────────────────────────────────────

describe('syncSessionIdentity', () => {
  it('updates both the ref and React state sink to the same session id', () => {
    const sessionIdRef = { current: 'sess-old' };
    const setCurrentSessionId = vi.fn();

    const result = syncSessionIdentity('sess-new', sessionIdRef, setCurrentSessionId);

    expect(result).toBe('sess-new');
    expect(sessionIdRef.current).toBe('sess-new');
    expect(setCurrentSessionId).toHaveBeenCalledTimes(1);
    expect(setCurrentSessionId).toHaveBeenCalledWith('sess-new');
  });
});
