import { useEffect, useState } from 'react';
import { AppMode } from '../types';

const MODE_META: Record<AppMode, { icon: string; label: string; gradient: string }> = {
  chat: { icon: '💬', label: 'CHAT', gradient: 'from-zinc-700 to-zinc-600' },
  plan: { icon: '🗺', label: 'PLAN', gradient: 'from-violet-600 to-violet-500' },
  code: { icon: '⚡', label: 'CODE', gradient: 'from-orange-600 to-orange-500' },
  debug: { icon: '🔍', label: 'DEBUG', gradient: 'from-emerald-600 to-emerald-500' },
  test: { icon: '✅', label: 'TEST', gradient: 'from-sky-600 to-sky-500' },
  review: { icon: '🔎', label: 'REVIEW', gradient: 'from-rose-600 to-rose-500' },
  qa: { icon: '🏁', label: 'QA', gradient: 'from-amber-600 to-amber-500' },
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
      <div className={`flex items-center gap-2.5 px-5 py-2 rounded-full shadow-xl
                       bg-gradient-to-r ${meta.gradient} text-white`}>
        <span className="text-base">{meta.icon}</span>
        <span className="text-xs font-bold tracking-widest">{meta.label}</span>
        <span className="text-xs opacity-70">activated</span>
        <span className="w-1.5 h-1.5 rounded-full bg-white animate-pulse" />
      </div>
    </div>
  );
}
