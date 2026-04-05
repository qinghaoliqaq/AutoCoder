import type { AppMode } from './types';

const TRAILING_INVOKE_RE = /(?:^|\n)<invoke\s+([\s\S]*?)\/>\s*$/;

/** Parse an attribute string like `key="val"` supporting `\"` escapes. */
export function parseAttr(attrs: string, name: string): string | undefined {
  const re = new RegExp(`${name}="((?:[^"\\\\]|\\\\.)*)"`);
  const m = attrs.match(re);
  if (!m) return undefined;
  return m[1].replace(/\\(.)/g, '$1');
}

export function parseInvoke(text: string): { skill: AppMode; task: string } | null {
  const match = text.match(TRAILING_INVOKE_RE);
  if (!match) return null;
  const attrs = match[1];
  const skill = parseAttr(attrs, 'skill') as AppMode | undefined;
  if (!skill || !['plan', 'code', 'debug', 'test', 'review', 'qa'].includes(skill)) return null;
  const task = parseAttr(attrs, 'task') ?? '';
  return { skill, task };
}

export function stripInvoke(text: string): string {
  return text.replace(TRAILING_INVOKE_RE, '').trimEnd();
}
