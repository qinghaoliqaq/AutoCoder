import { useState, useRef, useEffect } from 'react';
import { AGENT_PROVIDERS } from '../types';
import { ProviderIcon } from './icons/ProviderIcons';
import { ChevronDown } from 'lucide-react';

interface ProviderSelectProps {
  value: string;
  onChange: (value: string) => void;
}

export default function ProviderSelect({ value, onChange }: ProviderSelectProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  // Close on outside click
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open]);

  // Close on Escape
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
        className="flex w-full items-center gap-2.5 rounded-xl border border-zinc-200/80 bg-white/70 px-3 py-2.5 text-left text-sm text-zinc-800 outline-none transition focus:border-violet-300 dark:border-zinc-700 dark:bg-zinc-950/60 dark:text-zinc-100"
      >
        <ProviderIcon provider={value} size={18} />
        <span className="flex-1 truncate">{selected?.label ?? '未配置 (Not configured)'}</span>
        <ChevronDown className={`h-3.5 w-3.5 text-zinc-400 transition-transform ${open ? 'rotate-180' : ''}`} />
      </button>

      {open && (
        <div className="absolute left-0 right-0 top-full z-50 mt-1 max-h-64 overflow-y-auto rounded-xl border border-zinc-200/80 bg-white/95 shadow-xl backdrop-blur-xl dark:border-zinc-700 dark:bg-zinc-900/95">
          {AGENT_PROVIDERS.map((p) => (
            <button
              key={p.value}
              type="button"
              onClick={() => { onChange(p.value); setOpen(false); }}
              className={`flex w-full items-center gap-2.5 px-3 py-2 text-left text-sm transition-colors hover:bg-violet-50/80 dark:hover:bg-violet-500/10 ${
                p.value === value ? 'bg-violet-50/60 text-violet-700 dark:bg-violet-500/15 dark:text-violet-300' : 'text-zinc-700 dark:text-zinc-200'
              }`}
            >
              <ProviderIcon provider={p.value} size={18} />
              <span className="truncate">{p.label}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
