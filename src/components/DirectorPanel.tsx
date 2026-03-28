import { AppMode, ConfigStatus, MODES } from '../types';

type AnalysisState =
  | { status: 'idle' }
  | { status: 'thinking' }
  | { status: 'done'; decision: { mode: AppMode; reasoning: string; refined_task: string } }
  | { status: 'error'; message: string };

interface DirectorPanelProps {
  analysisState: AnalysisState;
  configStatus: ConfigStatus | null;
}

const MODE_COLORS: Record<AppMode, string> = {
  chat: 'text-zinc-600 dark:text-zinc-300 border-zinc-200 dark:border-zinc-700 bg-zinc-100 dark:bg-zinc-800',
  plan: 'text-violet-600 dark:text-violet-400 border-violet-200 dark:border-violet-500/40 bg-violet-50 dark:bg-violet-500/10',
  code: 'text-orange-600 dark:text-orange-400 border-orange-200 dark:border-orange-500/40 bg-orange-50 dark:bg-orange-500/10',
  debug: 'text-emerald-600 dark:text-emerald-400 border-emerald-200 dark:border-emerald-500/40 bg-emerald-50 dark:bg-emerald-500/10',
  test:   'text-sky-600 dark:text-sky-400 border-sky-200 dark:border-sky-500/40 bg-sky-50 dark:bg-sky-500/10',
  review: 'text-rose-600 dark:text-rose-400 border-rose-200 dark:border-rose-500/40 bg-rose-50 dark:bg-rose-500/10',
};

function ModeTag({ mode }: { mode: AppMode }) {
  const cfg = MODES.find((m) => m.id === mode);
  const icon = cfg?.icon ?? '💬';
  const label = cfg?.label ?? 'CHAT';
  return (
    <span className={`inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-semibold
                      border ${MODE_COLORS[mode]}`}>
      {icon} {label.toUpperCase()}
    </span>
  );
}

export default function DirectorPanel({ analysisState, configStatus }: DirectorPanelProps) {
  return (
    <div className="glass-panel px-5 py-4 flex-shrink-0">
      <div className="flex items-start gap-3">
        {/* Director avatar */}
        <div className="flex-shrink-0 w-8 h-8 rounded-xl bg-violet-100 dark:bg-violet-600/20 border border-violet-200 dark:border-violet-500/30
                        flex items-center justify-center text-sm font-bold text-violet-600 dark:text-violet-300 mt-0.5 shadow-sm">
          D
        </div>

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 mb-2 flex-wrap">
            <span className="text-sm font-semibold text-violet-700 dark:text-violet-300">Director</span>
            {configStatus?.configured ? (
              <>
                <span className="text-xs text-zinc-500">{configStatus.model}</span>
                <span className={`text-xs px-1.5 py-0.5 rounded font-mono font-medium
                  ${configStatus.api_format === 'anthropic'
                    ? 'bg-orange-50 text-orange-600 border border-orange-200 dark:bg-orange-500/15 dark:text-orange-400 dark:border-orange-500/30'
                    : 'bg-emerald-50 text-emerald-600 border border-emerald-200 dark:bg-emerald-500/15 dark:text-emerald-400 dark:border-emerald-500/30'}`}>
                  {configStatus.api_format}
                </span>
                <span className="text-xs text-zinc-400 dark:text-zinc-600 truncate max-w-[180px]" title={configStatus.base_url}>
                  {configStatus.base_url}
                </span>
              </>
            ) : configStatus ? (
              <span className="status-badge missing">Unconfigured</span>
            ) : null}
          </div>

          {/* States */}
          {analysisState.status === 'idle' && (
            <p className="text-xs text-zinc-500 dark:text-zinc-400">
              {configStatus?.configured
                ? 'Waiting for input, will automatically analyze and select mode…'
                : 'Please configure Director LLM in config.toml.'}
            </p>
          )}

          {analysisState.status === 'thinking' && (
            <div className="flex items-center gap-2 text-sm text-violet-500 dark:text-violet-400">
              <span className="pulse-dot bg-violet-400" />
              <span className="pulse-dot bg-violet-400" style={{ animationDelay: '0.25s' }} />
              <span className="pulse-dot bg-violet-400" style={{ animationDelay: '0.5s' }} />
              <span className="text-xs ml-1 font-medium text-violet-500 dark:text-violet-400">Analyzing intent…</span>
            </div>
          )}

          {analysisState.status === 'done' && (
            <div className="animate-slide-up space-y-2">
              <div className="flex items-center gap-3 flex-wrap">
                <ModeTag mode={analysisState.decision.mode} />
                <p className="text-xs text-zinc-500 dark:text-zinc-400 italic">
                  {analysisState.decision.reasoning}
                </p>
              </div>
              {analysisState.decision.refined_task && (
                <p className="text-xs text-zinc-600 dark:text-zinc-400 font-mono bg-zinc-50 dark:bg-zinc-800/50 rounded px-2 py-1.5 truncate border border-zinc-200 dark:border-zinc-800">
                  → {analysisState.decision.refined_task}
                </p>
              )}
            </div>
          )}

          {analysisState.status === 'error' && (
            <div className="animate-slide-up space-y-2">
              <p className="text-xs text-red-600 dark:text-red-400 font-medium">
                ⚠ Director Unavailable — Process Terminated
              </p>
              <p className="text-xs text-zinc-700 dark:text-zinc-400 font-mono bg-red-50 dark:bg-zinc-900 border border-red-100 dark:border-zinc-800 rounded px-3 py-2 leading-relaxed">
                {analysisState.message}
              </p>
              <div className="text-xs text-zinc-500 space-y-1 mt-2">
                <p className="font-medium text-zinc-600 dark:text-zinc-400">Fix methods (choose one):</p>
                <div className="pl-2 space-y-0.5 border-l-2 border-zinc-200 dark:border-zinc-700">
                  <p>1. Edit <code className="text-violet-600 dark:text-violet-300 font-semibold bg-zinc-100 dark:bg-zinc-800 px-1 rounded">config.toml</code> in project root</p>
                  <p>2. Set env <code className="text-violet-600 dark:text-violet-300 font-semibold bg-zinc-100 dark:bg-zinc-800 px-1 rounded">DIRECTOR_API_KEY</code> / <code className="text-violet-600 dark:text-violet-300 font-semibold bg-zinc-100 dark:bg-zinc-800 px-1 rounded">DIRECTOR_BASE_URL</code></p>
                  <p>3. Restart application to apply changes</p>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export type { AnalysisState };
