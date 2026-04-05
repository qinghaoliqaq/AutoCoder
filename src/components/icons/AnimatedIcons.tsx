import { FolderOpen, MessageCircleMore, Sparkles } from 'lucide-react';

type IconProps = {
  className?: string;
};

export const AnimatedMessageIcon = ({ className = '' }: IconProps) => (
  <div className={`hero-icon hero-icon-message ${className}`} aria-hidden="true">
    <span className="hero-icon__halo" />
    <MessageCircleMore className="hero-icon__glyph" strokeWidth={1.8} />
    <span className="hero-icon__dots">
      <span />
      <span />
      <span />
    </span>
  </div>
);

export const AnimatedFolderIcon = ({ className = '' }: IconProps) => (
  <div className={`hero-icon hero-icon-folder ${className}`} aria-hidden="true">
    <span className="hero-icon__halo" />
    <FolderOpen className="hero-icon__glyph" strokeWidth={1.85} />
  </div>
);

export const AnimatedSparklesIcon = ({ className = '' }: IconProps) => (
  <div className={`hero-icon hero-icon-sparkles ${className}`} aria-hidden="true">
    <span className="hero-icon__halo" />
    <Sparkles className="hero-icon__glyph" strokeWidth={1.8} />
  </div>
);
