import { useState, useRef, useEffect } from 'react';
import { AGENT_PROVIDERS } from '../types';
import { ProviderIcon } from './icons/ProviderIcons';
import { ChevronDown } from 'lucide-react';

interface ProviderSelectProps {
  value: string;
  onChange: (value: string) => void;
}

/** Brand colours for provider icons (used as inline color, not CSS variable) */
const PROVIDER_COLORS: Record<string, string> = {
  '':            '#A1A1AA',
  'anthropic':   '#FF6B35',
  'openai':      '#10A37F',
  'codex':       '#10A37F',
  'deepseek':    '#5C9DF5',
  'zhipu':       '#7B61FF',
  'glm':         '#7B61FF',
  'chatglm':     '#7B61FF',
  'minimax':     '#00D4AA',
  'moonshot':    '#1DB4E7',
  'kimi':        '#1DB4E7',
  'yi':          '#FFD000',
  '01ai':        '#FFD000',
  'baichuan':    '#5C6CF5',
  'qwen':        '#7B61FF',
  'dashscope':   '#7B61FF',
  'tongyi':      '#7B61FF',
  'groq':        '#E73C1E',
  'together':    '#9B59B6',
  'fireworks':   '#F97316',
  'siliconflow': '#0084FF',
};

export function ProviderIconColor({ provider, size = 18 }: { provider: string; size?: number }) {
  const color = PROVIDER_COLORS[provider.toLowerCase()] ?? '#A1A1AA';
  return (
    <span style={{ display: 'flex', color }}>
      <ProviderIcon provider={provider} size={size} />
    </span>
  );
}

export default function ProviderSelect({ value, onChange }: ProviderSelectProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    document.addEventListener('keydown', handler);
    return () => document.removeEventListener('keydown', handler);
  }, [open]);

  const selected = AGENT_PROVIDERS.find((p) => p.value === value);

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="flex w-full items-center gap-2 rounded-xl border border-edge-primary/60 bg-surface-input px-3 py-2.5 text-left text-sm text-content-primary outline-none transition-all duration-200 focus:border-themed-accent/50 focus:ring-2 focus:ring-themed-accent/10"
      >
        <ProviderIconColor provider={value} size={18} />
        <span className="flex-1 truncate">{selected?.label ?? '未配置 (Not configured)'}</span>
        <ChevronDown
          className="h-3.5 w-3.5 text-content-tertiary transition-transform"
          style={{ transform: open ? 'rotate(180deg)' : 'rotate(0deg)' }}
        />
      </button>

      {open && (
        <div className="absolute left-0 right-0 top-full z-50 mt-1 max-h-64 overflow-y-auto rounded-xl border border-edge-primary/60 bg-surface-elevated shadow-xl">
          {AGENT_PROVIDERS.map((p) => (
            <button
              key={p.value}
              type="button"
              onClick={() => { onChange(p.value); setOpen(false); }}
              className={`flex w-full items-center gap-2.5 px-3 py-2 text-left text-sm transition-colors ${
                p.value === value
                  ? 'bg-themed-accent-soft/70 text-themed-accent-text'
                  : 'text-content-secondary hover:bg-surface-tertiary/40 hover:text-content-primary'
              }`}
            >
              <ProviderIconColor provider={p.value} size={18} />
              <span className="truncate">{p.label}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
