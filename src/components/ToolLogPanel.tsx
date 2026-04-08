import { useRef, useEffect, useState } from 'react';
import { ToolLog } from '../types';

interface Props {
  logs: ToolLog[];
  onClose: () => void;
}

const AGENT_CONFIG: Record<string, { color: string; bg: string; label: string }> = {
  claude: {
    color: 'text-[#cc785c]',
    bg: 'bg-orange-500/10 border-orange-500/20',
    label: 'C',
  },
  codex: {
    color: 'text-[#10a37f]',
    bg: 'bg-emerald-500/10 border-emerald-500/20',
    label: 'X',
  },
  system: {
    color: 'text-violet-500',
    bg: 'bg-violet-500/10 border-violet-500/20',
    label: 'S',
  },
};

const TOOL_META: Record<string, { icon: string; color: string }> = {
  Shell: { icon: '>', color: 'text-emerald-500' },
  Bash: { icon: '$', color: 'text-emerald-500' },
  bash: { icon: '$', color: 'text-emerald-500' },
  VendoredSkill: { icon: '\u2726', color: 'text-violet-500' },
  Read: { icon: '\u2687', color: 'text-sky-500' },
  Write: { icon: '\u270E', color: 'text-amber-500' },
  Edit: { icon: '\u0394', color: 'text-amber-500' },
  Glob: { icon: '*', color: 'text-pink-500' },
  Grep: { icon: '/', color: 'text-pink-500' },
  StructuredAcceptance: { icon: '\u2713', color: 'text-violet-500' },
};

function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString('en-US', {
    hour: 'numeric',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  });
}

function ToolLogEntry({ log, index }: { log: ToolLog; index: number }) {
  const [expanded, setExpanded] = useState(false);
  const agentCfg = AGENT_CONFIG[log.agent] ?? { color: 'text-zinc-500', bg: 'bg-zinc-500/10 border-zinc-500/20', label: '?' };
  const toolMeta = TOOL_META[log.tool] ?? { icon: '\u2699', color: 'text-zinc-400' };
  const isLong = (log.input?.length ?? 0) > 120;

  return (
    <div
      className="group rounded-xl border border-edge-primary/50 bg-surface-secondary/50 px-3 py-2.5
                 backdrop-blur-sm transition-all duration-200
                 hover:border-edge-primary/70 hover:bg-surface-secondary/70 hover:shadow-sm"
      style={{ animationDelay: `${Math.min(index * 30, 300)}ms` }}
    >
      <div className="flex items-start gap-2.5">
        {/* Agent avatar */}
        <div className={`flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-lg border text-[10px] font-bold ${agentCfg.bg}`}>
          <span className={agentCfg.color}>{agentCfg.label}</span>
        </div>

        <div className="min-w-0 flex-1">
          {/* Header row */}
          <div className="flex items-center gap-2">
            <span className={`text-sm font-bold leading-none ${toolMeta.color}`}>
              {toolMeta.icon}
            </span>
            <span className="text-[11px] font-semibold text-content-primary">
              {log.tool}
            </span>
            <span className="ml-auto text-[10px] tabular-nums text-content-tertiary">
              {formatTime(log.timestamp)}
            </span>
          </div>

          {/* Input content */}
          {log.input && (
            <div className="mt-1.5">
              <pre
                className={`whitespace-pre-wrap break-all font-mono text-[10.5px] leading-[1.6] text-content-secondary
                           ${!expanded && isLong ? 'line-clamp-2' : ''}`}
              >
                {log.input}
              </pre>
              {isLong && (
                <button
                  onClick={() => setExpanded(v => !v)}
                  className="mt-1 text-[10px] font-medium text-violet-500 transition-colors hover:text-violet-700 dark:text-violet-400 dark:hover:text-violet-300"
                >
                  {expanded ? 'Collapse' : 'Show more...'}
                </button>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default function ToolLogPanel({ logs, onClose }: Props) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [logs]);

  return (
    <div className="flex flex-col h-full w-full overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-5 py-3 border-b
                      border-edge-primary/40 flex-shrink-0 min-h-[48px]"
           style={{ backgroundColor: 'rgb(var(--bg-secondary) / 0.2)' }}>
        <div className="flex items-center gap-2.5">
          <span className="text-[11px] font-bold uppercase tracking-widest text-content-primary select-none whitespace-nowrap">
            Tool Log
          </span>
          {logs.length > 0 && (
            <span className="rounded-full bg-surface-tertiary/80 px-1.5 py-0.5 text-[10px] font-semibold tabular-nums text-content-secondary">
              {logs.length}
            </span>
          )}
        </div>
        <button
          onClick={onClose}
          className="flex h-6 w-6 items-center justify-center rounded-md text-content-tertiary
                     transition-colors hover:bg-surface-tertiary/50 hover:text-content-primary"
        >
          <svg className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      {/* Log entries */}
      <div className="flex-1 overflow-y-auto px-3 py-2.5 space-y-2 min-h-0 custom-scrollbar">
        {logs.length === 0 && (
          <div className="flex flex-col items-center justify-center h-full px-4 text-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl border border-zinc-200/60 bg-zinc-100/60 dark:border-zinc-800 dark:bg-zinc-900/60">
              <span className="text-lg text-zinc-400 dark:text-zinc-600">$</span>
            </div>
            <div>
              <p className="text-[11px] font-medium text-zinc-500 dark:text-zinc-500">No tool calls yet</p>
              <p className="mt-0.5 text-[10px] text-zinc-400 dark:text-zinc-600">
                Tool executions will appear here in real-time
              </p>
            </div>
          </div>
        )}
        {logs.map((log, i) => (
          <ToolLogEntry key={i} log={log} index={i} />
        ))}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
