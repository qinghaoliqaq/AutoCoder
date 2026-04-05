/// Re-exports all provider icons and provides a unified lookup component.
/// SVG paths sourced from lobehub/lobe-icons (official brand logos).

export { default as AnthropicIcon } from './providers/Anthropic';
export { default as OpenAIIcon } from './providers/OpenAI';
export { default as DeepSeekIcon } from './providers/DeepSeek';
export { default as ZhipuIcon } from './providers/Zhipu';
export { default as ChatGLMIcon } from './providers/ChatGLM';
export { default as MiniMaxIcon } from './providers/MiniMax';
export { default as MoonshotIcon } from './providers/Moonshot';
export { default as KimiIcon } from './providers/Kimi';
export { default as YiIcon } from './providers/Yi';
export { default as BaichuanIcon } from './providers/Baichuan';
export { default as QwenIcon } from './providers/Qwen';
export { default as GroqIcon } from './providers/Groq';
export { default as TogetherIcon } from './providers/Together';
export { default as FireworksIcon } from './providers/Fireworks';
export { default as SiliconCloudIcon } from './providers/SiliconCloud';

import AnthropicIcon from './providers/Anthropic';
import OpenAIIcon from './providers/OpenAI';
import DeepSeekIcon from './providers/DeepSeek';
import ZhipuIcon from './providers/Zhipu';
import ChatGLMIcon from './providers/ChatGLM';
import MiniMaxIcon from './providers/MiniMax';
import MoonshotIcon from './providers/Moonshot';
import KimiIcon from './providers/Kimi';
import YiIcon from './providers/Yi';
import BaichuanIcon from './providers/Baichuan';
import QwenIcon from './providers/Qwen';
import GroqIcon from './providers/Groq';
import TogetherIcon from './providers/Together';
import FireworksIcon from './providers/Fireworks';
import SiliconCloudIcon from './providers/SiliconCloud';

interface IconProps {
  className?: string;
  size?: number;
}

function UnconfiguredIcon({ className, size = 20 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" className={className}>
      <rect width="20" height="20" rx="6" fill="#D4D4D8" />
      <text x="10" y="14.5" textAnchor="middle" fill="#71717A" fontSize="12" fontWeight="600" fontFamily="system-ui, sans-serif">?</text>
    </svg>
  );
}

const ICON_MAP: Record<string, (props: IconProps) => JSX.Element> = {
  '':            UnconfiguredIcon,
  'anthropic':   AnthropicIcon,
  'openai':      OpenAIIcon,
  'codex':       OpenAIIcon,
  'deepseek':    DeepSeekIcon,
  'zhipu':       ZhipuIcon,
  'glm':         ZhipuIcon,
  'chatglm':     ChatGLMIcon,
  'minimax':     MiniMaxIcon,
  'moonshot':    MoonshotIcon,
  'kimi':        KimiIcon,
  'yi':          YiIcon,
  '01ai':        YiIcon,
  'baichuan':    BaichuanIcon,
  'qwen':        QwenIcon,
  'dashscope':   QwenIcon,
  'tongyi':      QwenIcon,
  'groq':        GroqIcon,
  'together':    TogetherIcon,
  'fireworks':   FireworksIcon,
  'siliconflow': SiliconCloudIcon,
};

/** Get the icon component for a provider name. Falls back to "?" placeholder. */
export function ProviderIcon({ provider, className, size }: IconProps & { provider: string }) {
  const Icon = ICON_MAP[provider.toLowerCase()] ?? UnconfiguredIcon;
  return <Icon className={className} size={size} />;
}

export default ProviderIcon;
