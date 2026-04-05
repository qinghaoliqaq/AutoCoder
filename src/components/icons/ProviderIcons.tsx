/// Provider brand icons for the agent config UI.
/// Each icon is a ~20×20 SVG designed to be recognizable at small sizes.
/// Uses official brand colors where possible.

interface IconProps {
  className?: string;
  size?: number;
}

// Helper: colored circle with text abbreviation
function Badge({ letter, bg, fg = '#fff', className, size = 20 }: IconProps & { letter: string; bg: string; fg?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" className={className}>
      <rect width="20" height="20" rx="6" fill={bg} />
      <text x="10" y="14.5" textAnchor="middle" fill={fg} fontSize="11" fontWeight="700" fontFamily="system-ui, sans-serif">{letter}</text>
    </svg>
  );
}

// ── Anthropic ───────────────────────────────────────────────────────────────
export function AnthropicIcon({ className, size = 20 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" className={className}>
      <rect width="20" height="20" rx="6" fill="#191919" />
      <path d="M11.5 5L15.5 15H13.2L12.3 12.5H8.7L11.5 5Z" fill="#D4A27F" />
      <path d="M8.5 5L4.5 15H6.8L7.7 12.5H8.7L8.5 5Z" fill="#D4A27F" />
    </svg>
  );
}

// ── OpenAI ──────────────────────────────────────────────────────────────────
export function OpenAIIcon({ className, size = 20 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" className={className}>
      <rect width="20" height="20" rx="6" fill="#000" />
      <path d="M10 4C7.5 4 5.8 5.8 5.8 7.5c0 .8.3 1.5.8 2.1-.5.6-.8 1.3-.8 2.1C5.8 13.5 7.2 15 9 15.6v-1.2c-1.2-.4-2-1.5-2-2.7 0-.6.2-1.1.5-1.5.5.3 1 .5 1.5.5s1-.2 1.5-.5c.3.4.5.9.5 1.5 0 1.2-.8 2.3-2 2.7v1.2c1.8-.6 3.2-2.1 3.2-3.9 0-.8-.3-1.5-.8-2.1.5-.6.8-1.3.8-2.1C12.2 5.2 10.5 4 10 4z" fill="#fff" />
    </svg>
  );
}

// ── DeepSeek ────────────────────────────────────────────────────────────────
export function DeepSeekIcon({ className, size = 20 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" className={className}>
      <rect width="20" height="20" rx="6" fill="#4D6BFE" />
      <text x="10" y="14.5" textAnchor="middle" fill="#fff" fontSize="10" fontWeight="800" fontFamily="system-ui, sans-serif">DS</text>
    </svg>
  );
}

// ── 智谱 Zhipu / GLM ───────────────────────────────────────────────────────
export function ZhipuIcon({ className, size = 20 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" className={className}>
      <rect width="20" height="20" rx="6" fill="#1A56DB" />
      <text x="10" y="14" textAnchor="middle" fill="#fff" fontSize="9.5" fontWeight="700" fontFamily="system-ui, sans-serif">智谱</text>
    </svg>
  );
}

// ── MiniMax ─────────────────────────────────────────────────────────────────
export function MiniMaxIcon({ className, size = 20 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" className={className}>
      <rect width="20" height="20" rx="6" fill="#FF6B35" />
      <text x="10" y="14.5" textAnchor="middle" fill="#fff" fontSize="8" fontWeight="800" fontFamily="system-ui, sans-serif">MM</text>
    </svg>
  );
}

// ── 月之暗面 Moonshot ───────────────────────────────────────────────────────
export function MoonshotIcon({ className, size = 20 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" className={className}>
      <rect width="20" height="20" rx="6" fill="#1C1C2E" />
      <circle cx="10" cy="10" r="5" fill="none" stroke="#A78BFA" strokeWidth="1.5" />
      <path d="M8 10a4 4 0 0 1 4-4" fill="none" stroke="#FBBF24" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  );
}

// ── 零一万物 Yi ─────────────────────────────────────────────────────────────
export function YiIcon({ className, size = 20 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" className={className}>
      <rect width="20" height="20" rx="6" fill="#0D9488" />
      <text x="10" y="14.5" textAnchor="middle" fill="#fff" fontSize="11" fontWeight="800" fontFamily="system-ui, sans-serif">01</text>
    </svg>
  );
}

// ── 百川 Baichuan ───────────────────────────────────────────────────────────
export function BaichuanIcon({ className, size = 20 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" className={className}>
      <rect width="20" height="20" rx="6" fill="#2563EB" />
      <text x="10" y="14" textAnchor="middle" fill="#fff" fontSize="9.5" fontWeight="700" fontFamily="system-ui, sans-serif">百川</text>
    </svg>
  );
}

// ── 通义千问 Qwen ───────────────────────────────────────────────────────────
export function QwenIcon({ className, size = 20 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" className={className}>
      <rect width="20" height="20" rx="6" fill="#FF6A00" />
      <text x="10" y="14" textAnchor="middle" fill="#fff" fontSize="8" fontWeight="700" fontFamily="system-ui, sans-serif">千问</text>
    </svg>
  );
}

// ── Groq ────────────────────────────────────────────────────────────────────
export function GroqIcon({ className, size = 20 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" className={className}>
      <rect width="20" height="20" rx="6" fill="#F55036" />
      <text x="10" y="14.5" textAnchor="middle" fill="#fff" fontSize="8.5" fontWeight="800" fontFamily="system-ui, sans-serif">GQ</text>
    </svg>
  );
}

// ── Together AI ─────────────────────────────────────────────────────────────
export function TogetherIcon({ className, size = 20 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" className={className}>
      <rect width="20" height="20" rx="6" fill="#6366F1" />
      <circle cx="7.5" cy="10" r="2.5" fill="#fff" opacity="0.9" />
      <circle cx="12.5" cy="10" r="2.5" fill="#fff" opacity="0.6" />
    </svg>
  );
}

// ── Fireworks AI ────────────────────────────────────────────────────────────
export function FireworksIcon({ className, size = 20 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" className={className}>
      <rect width="20" height="20" rx="6" fill="#7C3AED" />
      <path d="M10 5L11.5 9H8.5L10 5Z" fill="#FBBF24" />
      <path d="M10 15L8.5 11H11.5L10 15Z" fill="#FB923C" />
      <path d="M5 10L9 8.5V11.5L5 10Z" fill="#F87171" />
      <path d="M15 10L11 11.5V8.5L15 10Z" fill="#A78BFA" />
    </svg>
  );
}

// ── 硅基流动 SiliconFlow ────────────────────────────────────────────────────
export function SiliconFlowIcon({ className, size = 20 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" className={className}>
      <rect width="20" height="20" rx="6" fill="#0EA5E9" />
      <text x="10" y="14.5" textAnchor="middle" fill="#fff" fontSize="10" fontWeight="800" fontFamily="system-ui, sans-serif">SF</text>
    </svg>
  );
}

// ── Unconfigured placeholder ────────────────────────────────────────────────
export function UnconfiguredIcon({ className, size = 20 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" className={className}>
      <rect width="20" height="20" rx="6" fill="#D4D4D8" />
      <text x="10" y="14.5" textAnchor="middle" fill="#71717A" fontSize="12" fontWeight="600" fontFamily="system-ui, sans-serif">?</text>
    </svg>
  );
}

// ── Lookup ──────────────────────────────────────────────────────────────────

const ICON_MAP: Record<string, (props: IconProps) => JSX.Element> = {
  '':           UnconfiguredIcon,
  'anthropic':  AnthropicIcon,
  'openai':     OpenAIIcon,
  'codex':      OpenAIIcon,
  'deepseek':   DeepSeekIcon,
  'zhipu':      ZhipuIcon,
  'glm':        ZhipuIcon,
  'chatglm':    ZhipuIcon,
  'minimax':    MiniMaxIcon,
  'moonshot':   MoonshotIcon,
  'kimi':       MoonshotIcon,
  'yi':         YiIcon,
  '01ai':       YiIcon,
  'baichuan':   BaichuanIcon,
  'qwen':       QwenIcon,
  'dashscope':  QwenIcon,
  'tongyi':     QwenIcon,
  'groq':       GroqIcon,
  'together':   TogetherIcon,
  'fireworks':  FireworksIcon,
  'siliconflow': SiliconFlowIcon,
};

/** Get the icon component for a provider name. */
export function ProviderIcon({ provider, className, size }: IconProps & { provider: string }) {
  const Icon = ICON_MAP[provider.toLowerCase()] ?? (() => <Badge letter={provider.slice(0, 2).toUpperCase()} bg="#71717A" className={className} size={size} />);
  return <Icon className={className} size={size} />;
}

export default ProviderIcon;
