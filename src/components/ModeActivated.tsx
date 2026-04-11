import { useEffect, useState } from 'react';
import { AppMode } from '../types';
import {
  VscCommentDiscussion, VscMap, VscSymbolEvent, VscSearch,
  VscBeaker, VscShield, VscVerified, VscFile,
} from 'react-icons/vsc';

const MODE_META: Record<AppMode, { icon: React.ReactNode; label: string; color: string; bg: string }> = {
  chat: {
    icon: <VscCommentDiscussion className="h-3.5 w-3.5" />,
    label: 'CHAT',
    color: 'text-zinc-700 dark:text-zinc-200',
    bg: 'border-zinc-200/60 bg-white/80 dark:border-zinc-700/50 dark:bg-zinc-800/80',
  },
  plan: {
    icon: <VscMap className="h-3.5 w-3.5" />,
    label: 'PLAN',
    color: 'text-violet-600 dark:text-violet-400',
    bg: 'border-violet-200/60 bg-violet-50/80 dark:border-violet-500/30 dark:bg-violet-500/15',
  },
  code: {
    icon: <VscSymbolEvent className="h-3.5 w-3.5" />,
    label: 'CODE',
    color: 'text-orange-600 dark:text-orange-400',
    bg: 'border-orange-200/60 bg-orange-50/80 dark:border-orange-500/30 dark:bg-orange-500/15',
  },
  debug: {
    icon: <VscSearch className="h-3.5 w-3.5" />,
    label: 'DEBUG',
    color: 'text-emerald-600 dark:text-emerald-400',
    bg: 'border-emerald-200/60 bg-emerald-50/80 dark:border-emerald-500/30 dark:bg-emerald-500/15',
  },
  test: {
    icon: <VscBeaker className="h-3.5 w-3.5" />,
    label: 'TEST',
    color: 'text-sky-600 dark:text-sky-400',
    bg: 'border-sky-200/60 bg-sky-50/80 dark:border-sky-500/30 dark:bg-sky-500/15',
  },
  review: {
    icon: <VscShield className="h-3.5 w-3.5" />,
    label: 'REVIEW',
    color: 'text-rose-600 dark:text-rose-400',
    bg: 'border-rose-200/60 bg-rose-50/80 dark:border-rose-500/30 dark:bg-rose-500/15',
  },
  qa: {
    icon: <VscVerified className="h-3.5 w-3.5" />,
    label: 'QA',
    color: 'text-amber-600 dark:text-amber-400',
    bg: 'border-amber-200/60 bg-amber-50/80 dark:border-amber-500/30 dark:bg-amber-500/15',
  },
  document: {
    icon: <VscFile className="h-3.5 w-3.5" />,
    label: 'DOCUMENT',
    color: 'text-lime-600 dark:text-lime-400',
    bg: 'border-lime-200/60 bg-lime-50/80 dark:border-lime-500/30 dark:bg-lime-500/15',
  },
};

export default function ModeActivated({ mode }: { mode: AppMode | null }) {
  const [visible, setVisible] = useState(false);
  const [active, setActive] = useState<AppMode | null>(null);

  useEffect(() => {
    if (!mode || mode === 'chat') return;
    setActive(mode);
    setVisible(true);
    const t = setTimeout(() => setVisible(false), 2800);
    return () => clearTimeout(t);
  }, [mode]);

  if (!active) return null;

  const meta = MODE_META[active];

  return (
    <div
      className={`fixed top-14 inset-x-0 flex justify-center z-50 pointer-events-none
                  transition-all duration-500 ease-out
                  ${visible ? 'opacity-100 translate-y-0' : 'opacity-0 -translate-y-3'}`}
    >
      <div className={`flex items-center gap-2 px-4 py-2 rounded-full border backdrop-blur-xl shadow-lg ${meta.bg}`}>
        <span className={meta.color}>{meta.icon}</span>
        <span className={`text-[11px] font-bold tracking-widest ${meta.color}`}>{meta.label}</span>
        <span className="text-[10px] text-zinc-400 dark:text-zinc-500">activated</span>
        <span className={`w-1.5 h-1.5 rounded-full animate-pulse ${meta.color.replace('text-', 'bg-')}`} />
      </div>
    </div>
  );
}
