/**
 * Role identity icons for agent cards.
 * These represent the app-level roles (Claude / Codex),
 * independent of the underlying API provider.
 */

type IconProps = { size?: number; className?: string };

/**
 * Claude (primary agent) — four-point starburst mark.
 * Warm orange-amber gradient.
 */
export function ClaudeRoleIcon({ size = 24, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" className={className} aria-hidden="true">
      <defs>
        <linearGradient id="role-claude" x1="2" y1="2" x2="22" y2="22" gradientUnits="userSpaceOnUse">
          <stop stopColor="#FB923C" />
          <stop offset="1" stopColor="#F59E0B" />
        </linearGradient>
      </defs>
      <path
        d="M12 2C12.6 9 15 11.4 22 12C15 12.6 12.6 15 12 22C11.4 15 9 12.6 2 12C9 11.4 11.4 9 12 2Z"
        fill="url(#role-claude)"
      />
    </svg>
  );
}

/**
 * Codex (secondary agent) — hexagonal prism mark.
 * Cool emerald-teal gradient.
 */
export function CodexRoleIcon({ size = 24, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" className={className} aria-hidden="true">
      <defs>
        <linearGradient id="role-codex" x1="2" y1="2" x2="22" y2="22" gradientUnits="userSpaceOnUse">
          <stop stopColor="#34D399" />
          <stop offset="1" stopColor="#14B8A6" />
        </linearGradient>
      </defs>
      <path
        d="M12 3L20.66 8V16L12 21L3.34 16V8L12 3Z"
        fill="url(#role-codex)"
      />
      <path
        d="M12 7L16.33 9.5V14.5L12 17L7.67 14.5V9.5L12 7Z"
        fill="white"
        opacity="0.2"
      />
    </svg>
  );
}
