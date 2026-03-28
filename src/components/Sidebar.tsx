import { AppMode, MODES, SystemStatus } from '../types';

interface SidebarProps {
  mode: AppMode | null;   // null = not yet decided by Director
  status: SystemStatus | null;
  checking: boolean;
}

const MODE_ACTIVE_STYLE: Record<AppMode, string> = {
  chat: 'bg-zinc-200/50 dark:bg-zinc-800/80 border-zinc-300 dark:border-zinc-700',
  plan: 'bg-violet-100 dark:bg-violet-500/20 border-violet-200 dark:border-violet-500/30 text-violet-600 dark:text-violet-400',
  code: 'bg-orange-100 dark:bg-orange-500/20 border-orange-200 dark:border-orange-500/30 text-orange-600 dark:text-orange-400',
  debug: 'bg-emerald-100 dark:bg-emerald-500/20 border-emerald-200 dark:border-emerald-500/30 text-emerald-600 dark:text-emerald-400',
  test:   'bg-sky-100 dark:bg-sky-500/20 border-sky-200 dark:border-sky-500/30 text-sky-600 dark:text-sky-400',
  review: 'bg-rose-100 dark:bg-rose-500/20 border-rose-200 dark:border-rose-500/30 text-rose-600 dark:text-rose-400',
};

export default function Sidebar({ mode, status, checking }: SidebarProps) {
  return (
    <aside className="w-16 flex flex-col items-center py-4 gap-2 bg-zinc-100/50 dark:bg-[#18181a] border-r border-zinc-200 dark:border-zinc-800/80">
      {/* Logo */}
      <div className="w-10 h-10 rounded-xl bg-violet-100 dark:bg-violet-600/20 border border-violet-200 dark:border-violet-500/30
                      flex items-center justify-center text-lg mb-2 shadow-sm" title="AI Dev Hub">
        🧠
      </div>

      <div className="w-10 h-px bg-zinc-200 dark:bg-zinc-800 mb-2" />

      {/* Mode indicators — read-only, Director sets these */}
      {MODES.map((m) => {
        const isActive = mode === m.id;
        return (
          <div
            key={m.id}
            title={`${m.label} — ${m.description}${isActive ? ' (Active Mode)' : ''}`}
            className={`w-10 h-10 rounded-xl flex items-center justify-center text-lg
                        border transition-all duration-300
                        ${isActive
                ? MODE_ACTIVE_STYLE[m.id]
                : 'border-transparent text-zinc-400 dark:text-zinc-600 hover:bg-zinc-200/50 dark:hover:bg-zinc-800/50'}`}
          >
            {m.icon}
            {isActive && (
              <span className="sr-only">Active</span>
            )}
          </div>
        );
      })}

      <div className="flex-1" />

      {/* Tool status dots */}
      <div className="space-y-1.5 flex flex-col items-center pb-1">
        <div
          className={`w-2 h-2 rounded-full transition-colors ${checking ? 'bg-amber-400 animate-pulse' :
              status?.claude.installed ? 'bg-emerald-500' : 'bg-red-400'
            }`}
          title={`Claude: ${status?.claude.installed ? status.claude.version ?? 'Installed' : 'Missing'}`}
        />
        <div
          className={`w-2 h-2 rounded-full transition-colors ${checking ? 'bg-amber-400 animate-pulse' :
              status?.codex.installed ? 'bg-emerald-500' : 'bg-red-400'
            }`}
          title={`Codex: ${status?.codex.installed ? status.codex.version ?? 'Installed' : 'Missing'}`}
        />
      </div>
    </aside>
  );
}
