import { AppMode, ModeConfig, MODES, SystemStatus } from '../types';

interface ModeSelectorProps {
  current: AppMode;
  status: SystemStatus | null;
  onChange: (mode: AppMode) => void;
}

function isModeAvailable(mode: ModeConfig, status: SystemStatus | null): boolean {
  if (!status) return false;
  if (mode.requiresBoth) return status.claude.installed && status.codex.installed;
  if (mode.leader === 'claude') return status.claude.installed;
  if (mode.leader === 'codex') return status.codex.installed;
  return true;
}

export default function ModeSelector({ current, status, onChange }: ModeSelectorProps) {
  return (
    <div className="glass-panel p-5">
      <h2 className="text-xs font-bold text-zinc-800 dark:text-zinc-300 uppercase tracking-wider mb-4">
        Work Mode
      </h2>
      <div className="grid grid-cols-4 gap-3">
        {MODES.map((mode) => {
          const available = isModeAvailable(mode, status);
          const isActive = current === mode.id;

          return (
            <button
              key={mode.id}
              onClick={() => available && onChange(mode.id)}
              className={`mode-btn ${isActive ? 'active' : ''} ${!available ? 'disabled' : ''}`}
              title={available ? mode.description : 'Missing required tools'}
            >
              <span className="text-2xl mb-1">{mode.icon}</span>
              <div className="text-center">
                <div className={`font-semibold text-sm ${isActive ? 'text-violet-600 dark:text-violet-400' : 'text-zinc-800 dark:text-zinc-300'}`}>
                  {mode.label}
                </div>
                <div className={`text-[10px] uppercase font-bold tracking-wider mt-1 ${mode.color} opacity-80`}>
                  {mode.leader === 'director' ? 'Director' : mode.leader === 'claude' ? 'Claude' : 'Codex'}
                </div>
              </div>
              <p className="text-xs text-zinc-500 dark:text-zinc-400 text-center leading-relaxed hidden lg:block mt-1">
                {mode.description}
              </p>
              {!available && (
                <span className="text-[10px] font-bold uppercase tracking-wider text-red-500 dark:text-red-400 opacity-80 mt-1">Unavailable</span>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}
