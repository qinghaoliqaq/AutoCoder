/**
 * Hero section icons — Claude-inspired gradient SVGs with CSS animation.
 *
 * Design language:
 *   • Filled shapes with linear gradients (not stroke-only)
 *   • Organic curves, soft points
 *   • Small companion elements for visual depth
 *   • Animation handled via .hero-icon-* CSS classes in index.css
 */

type IconProps = { className?: string };

/* ── Starburst — main hero AI identity mark ────────────────────────── */

export const AnimatedMessageIcon = ({ className = '' }: IconProps) => (
  <div className={`hero-icon hero-icon-message ${className}`} aria-hidden="true">
    <svg className="hero-icon__glyph" viewBox="0 0 48 48" fill="none">
      <defs>
        <linearGradient id="hero-ai" x1="4" y1="4" x2="44" y2="44" gradientUnits="userSpaceOnUse">
          <stop stopColor="#A78BFA" />
          <stop offset="0.5" stopColor="#C084FC" />
          <stop offset="1" stopColor="#F472B6" />
        </linearGradient>
      </defs>
      {/* Primary 4-pointed starburst */}
      <path
        d="M24 2C25.2 16.5 31.5 22.8 46 24C31.5 25.2 25.2 31.5 24 46C22.8 31.5 16.5 25.2 2 24C16.5 22.8 22.8 16.5 24 2Z"
        fill="url(#hero-ai)"
      />
      {/* Small companion sparkle — top-right */}
      <path
        d="M38 4C38.5 9.8 40.2 11.5 46 12C40.2 12.5 38.5 14.2 38 20C37.5 14.2 35.8 12.5 30 12C35.8 11.5 37.5 9.8 38 4Z"
        fill="url(#hero-ai)"
        opacity="0.35"
      />
    </svg>
  </div>
);

/* ── Folder — open project card icon ───────────────────────────────── */

export const AnimatedFolderIcon = ({ className = '' }: IconProps) => (
  <div className={`hero-icon hero-icon-folder ${className}`} aria-hidden="true">
    <svg className="hero-icon__glyph" viewBox="0 0 24 24" fill="none">
      <defs>
        <linearGradient id="hero-folder" x1="2" y1="4" x2="22" y2="20" gradientUnits="userSpaceOnUse">
          <stop stopColor="#38BDF8" />
          <stop offset="1" stopColor="#3B82F6" />
        </linearGradient>
      </defs>
      {/* Folder back panel */}
      <path
        d="M4 5C2.9 5 2 5.9 2 7V19C2 20.1 2.9 21 4 21H20C21.1 21 22 20.1 22 19V9C22 7.9 21.1 7 20 7H12.4L10.4 5H4Z"
        fill="url(#hero-folder)"
        opacity="0.25"
      />
      {/* Folder front flap */}
      <path
        d="M2 10C2 8.9 2.9 8 4 8H20C21.1 8 22 8.9 22 10V19C22 20.1 21.1 21 20 21H4C2.9 21 2 20.1 2 19V10Z"
        fill="url(#hero-folder)"
      />
      {/* Document accent line */}
      <rect x="6" y="12" width="8" height="1.5" rx="0.75" fill="white" opacity="0.5" />
      <rect x="6" y="15.5" width="5" height="1.5" rx="0.75" fill="white" opacity="0.3" />
    </svg>
  </div>
);

/* ── Wand — start fresh card icon ──────────────────────────────────── */

export const AnimatedSparklesIcon = ({ className = '' }: IconProps) => (
  <div className={`hero-icon hero-icon-sparkles ${className}`} aria-hidden="true">
    <svg className="hero-icon__glyph" viewBox="0 0 24 24" fill="none">
      <defs>
        <linearGradient id="hero-wand" x1="2" y1="2" x2="22" y2="22" gradientUnits="userSpaceOnUse">
          <stop stopColor="#FBBF24" />
          <stop offset="1" stopColor="#F97316" />
        </linearGradient>
      </defs>
      {/* Wand body — diagonal rounded bar */}
      <rect
        x="2" y="13.5" width="15" height="2.8" rx="1.4"
        transform="rotate(-45 2 13.5)"
        fill="url(#hero-wand)"
        opacity="0.55"
      />
      {/* Star tip — 4-pointed sparkle */}
      <path
        d="M17 1C17.45 5.6 18.4 6.55 23 7C18.4 7.45 17.45 8.4 17 13C16.55 8.4 15.6 7.45 11 7C15.6 6.55 16.55 5.6 17 1Z"
        fill="url(#hero-wand)"
      />
      {/* Small accent sparkle */}
      <path
        d="M8 1.5C8.2 3.2 8.8 3.8 10.5 4C8.8 4.2 8.2 4.8 8 6.5C7.8 4.8 7.2 4.2 5.5 4C7.2 3.8 7.8 3.2 8 1.5Z"
        fill="url(#hero-wand)"
        opacity="0.4"
      />
    </svg>
  </div>
);
