import { SystemStatus } from '../types';
import { CheckCircle2, AlertTriangle } from 'lucide-react';

interface StatusPanelProps {
  status: SystemStatus | null;
  checking: boolean;
  onRecheck: () => void;
}

export default function StatusPanel({ status, checking, onRecheck }: StatusPanelProps) {
  const claudeOk = status?.claude?.installed;
  const codexOk = status?.codex?.installed;

  const allOk = claudeOk && codexOk;
  const someOk = claudeOk || codexOk;

  const statusColor = checking
    ? 'bg-amber-400'
    : allOk
      ? 'bg-emerald-500'
      : someOk
        ? 'bg-amber-500'
        : 'bg-red-500';

  const label = checking
    ? 'Checking...'
    : allOk
      ? 'All Tools Ready'
      : someOk
        ? 'Some Tools Missing'
        : 'Tools Missing';

  return (
    <div className="group relative flex items-center gap-2">
      <button
        onClick={onRecheck}
        disabled={checking}
        className="flex items-center gap-2 px-2.5 py-1.5 rounded-md hover:bg-zinc-100 dark:hover:bg-zinc-800 transition-colors border border-transparent hover:border-zinc-200 dark:hover:border-zinc-700"
      >
        <span className="relative flex h-2.5 w-2.5">
          {checking && <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-amber-400 opacity-75"></span>}
          <span className={`relative inline-flex rounded-full h-2.5 w-2.5 ${statusColor}`}></span>
        </span>
        <span className="text-xs font-medium text-zinc-600 dark:text-zinc-400">{label}</span>
      </button>

      {/* Hover tooltip with details */}
      <div className="absolute right-2 sm:right-6 top-full mt-2 w-64 p-3 bg-white dark:bg-zinc-900 border border-zinc-200 dark:border-zinc-800 rounded-xl shadow-xl opacity-0 invisible group-hover:opacity-100 group-hover:visible transition-all z-[100]">
        <h3 className="text-xs font-semibold text-zinc-800 dark:text-zinc-200 mb-2 uppercase tracking-wider">Environment</h3>

        <div className="space-y-2">
          <div className="flex items-center justify-between text-xs">
            <span className="text-zinc-600 dark:text-zinc-400">Claude Code</span>
            {claudeOk ? <CheckCircle2 className="w-3.5 h-3.5 text-emerald-500" /> : <AlertTriangle className="w-3.5 h-3.5 text-red-500" />}
          </div>
          <div className="flex items-center justify-between text-xs">
            <span className="text-zinc-600 dark:text-zinc-400">OpenAI Codex</span>
            {codexOk ? <CheckCircle2 className="w-3.5 h-3.5 text-emerald-500" /> : <AlertTriangle className="w-3.5 h-3.5 text-red-500" />}
          </div>
        </div>

        {(!claudeOk || !codexOk) && (
          <div className="mt-3 pt-3 border-t border-zinc-100 dark:border-zinc-800">
            <p className="text-[10px] text-zinc-500 dark:text-zinc-400 leading-relaxed">
              Some tools are missing. Please install them via npm globally to enable full functionality.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
