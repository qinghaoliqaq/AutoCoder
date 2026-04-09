import { useMemo } from 'react';
import { Terminal, FileText, FilePen, Pencil, Search, SearchCode, Sparkles, CheckCircle2, Wrench } from 'lucide-react';
import { ToolLog } from '../types';

interface Props {
  logs: ToolLog[];
  onClose: () => void;
}

const AGENT_CONFIG: Record<string, { color: string; bg: string; label: string }> = {
  claude: { color: 'text-[#cc785c]', bg: 'bg-orange-500/10 border-orange-500/20', label: 'Claude' },
  codex:  { color: 'text-[#10a37f]', bg: 'bg-emerald-500/10 border-emerald-500/20', label: 'Codex' },
  system: { color: 'text-violet-500', bg: 'bg-violet-500/10 border-violet-500/20', label: 'System' },
};

const TOOL_META: Record<string, { icon: React.ReactNode; color: string }> = {
  Shell:    { icon: <Terminal className="h-4 w-4" />, color: 'text-emerald-500' },
  Bash:     { icon: <Terminal className="h-4 w-4" />, color: 'text-emerald-500' },
  bash:     { icon: <Terminal className="h-4 w-4" />, color: 'text-emerald-500' },
  Read:     { icon: <FileText className="h-4 w-4" />, color: 'text-sky-500' },
  Write:    { icon: <FilePen className="h-4 w-4" />, color: 'text-amber-500' },
  Edit:     { icon: <Pencil className="h-4 w-4" />, color: 'text-amber-500' },
  Glob:     { icon: <Search className="h-4 w-4" />, color: 'text-pink-500' },
  Grep:     { icon: <SearchCode className="h-4 w-4" />, color: 'text-pink-500' },
  BundledSkill: { icon: <Sparkles className="h-4 w-4" />, color: 'text-themed-accent-text' },
  StructuredAcceptance: { icon: <CheckCircle2 className="h-4 w-4" />, color: 'text-themed-accent-text' },
};

const DEFAULT_TOOL_META = { icon: <Wrench className="h-4 w-4" />, color: 'text-content-tertiary' };

interface ToolStat {
  tool: string;
  count: number;
  icon: React.ReactNode;
  color: string;
}

interface AgentStat {
  agent: string;
  count: number;
  label: string;
  color: string;
  bg: string;
}

export default function ToolLogPanel({ logs, onClose }: Props) {
  const { toolStats, agentStats, totalCalls } = useMemo(() => {
    const toolMap = new Map<string, number>();
    const agentMap = new Map<string, number>();

    for (const log of logs) {
      // Normalize bash variants
      const toolName = log.tool === 'bash' || log.tool === 'Shell' ? 'Bash' : log.tool;
      toolMap.set(toolName, (toolMap.get(toolName) ?? 0) + 1);
      agentMap.set(log.agent, (agentMap.get(log.agent) ?? 0) + 1);
    }

    const toolStats: ToolStat[] = Array.from(toolMap.entries())
      .map(([tool, count]) => {
        const meta = TOOL_META[tool] ?? DEFAULT_TOOL_META;
        return { tool, count, icon: meta.icon, color: meta.color };
      })
      .sort((a, b) => b.count - a.count);

    const agentStats: AgentStat[] = Array.from(agentMap.entries())
      .map(([agent, count]) => {
        const cfg = AGENT_CONFIG[agent] ?? { color: 'text-zinc-500', bg: 'bg-zinc-500/10 border-zinc-500/20', label: agent };
        return { agent, count, label: cfg.label, color: cfg.color, bg: cfg.bg };
      })
      .sort((a, b) => b.count - a.count);

    return { toolStats, agentStats, totalCalls: logs.length };
  }, [logs]);

  return (
    <div className="flex flex-col h-full w-full overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-5 py-3 border-b
                      border-edge-primary/40 flex-shrink-0 min-h-[48px]"
           style={{ backgroundColor: 'rgb(var(--bg-secondary) / 0.2)' }}>
        <div className="flex items-center gap-2.5">
          <span className="text-[11px] font-bold uppercase tracking-widest text-content-primary select-none whitespace-nowrap">
            Tool Stats
          </span>
          {totalCalls > 0 && (
            <span className="rounded-full bg-surface-tertiary/80 px-1.5 py-0.5 text-[10px] font-semibold tabular-nums text-content-secondary">
              {totalCalls} calls
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

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-4 py-4 space-y-5 min-h-0 custom-scrollbar">
        {totalCalls === 0 ? (
          <div className="flex flex-col items-center justify-center h-full px-4 text-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl border border-edge-primary/60 bg-surface-tertiary/60">
              <Terminal className="h-5 w-5 text-content-tertiary" />
            </div>
            <div>
              <p className="text-[11px] font-medium text-content-secondary">No tool calls yet</p>
              <p className="mt-0.5 text-[10px] text-content-tertiary">
                Tool call statistics will appear here
              </p>
            </div>
          </div>
        ) : (
          <>
            {/* Agent breakdown */}
            <section>
              <h3 className="text-[10px] font-bold uppercase tracking-widest text-content-tertiary mb-2.5">By Agent</h3>
              <div className="space-y-2">
                {agentStats.map(stat => (
                  <div key={stat.agent} className="flex items-center gap-3">
                    <div className={`flex h-7 w-7 items-center justify-center rounded-lg border text-[10px] font-bold ${stat.bg}`}>
                      <span className={stat.color}>{stat.label[0]}</span>
                    </div>
                    <span className="text-[12px] font-medium text-content-primary flex-1">{stat.label}</span>
                    <div className="flex items-center gap-2">
                      <div className="w-20 h-1.5 rounded-full bg-surface-tertiary/60 overflow-hidden">
                        <div
                          className="h-full rounded-full transition-all duration-500"
                          style={{
                            width: `${Math.round((stat.count / totalCalls) * 100)}%`,
                            backgroundColor: `rgb(var(--accent))`,
                          }}
                        />
                      </div>
                      <span className="text-[11px] tabular-nums font-semibold text-content-secondary w-8 text-right">{stat.count}</span>
                    </div>
                  </div>
                ))}
              </div>
            </section>

            {/* Tool breakdown */}
            <section>
              <h3 className="text-[10px] font-bold uppercase tracking-widest text-content-tertiary mb-2.5">By Tool</h3>
              <div className="space-y-1.5">
                {toolStats.map(stat => (
                  <div key={stat.tool} className="flex items-center gap-2.5 py-1">
                    <span className={`flex items-center ${stat.color}`}>{stat.icon}</span>
                    <span className="text-[12px] font-medium text-content-primary flex-1">{stat.tool}</span>
                    <div className="flex items-center gap-2">
                      <div className="w-16 h-1.5 rounded-full bg-surface-tertiary/60 overflow-hidden">
                        <div
                          className="h-full rounded-full transition-all duration-500"
                          style={{
                            width: `${Math.round((stat.count / totalCalls) * 100)}%`,
                            backgroundColor: `rgb(var(--accent))`,
                          }}
                        />
                      </div>
                      <span className="text-[11px] tabular-nums font-semibold text-content-secondary w-8 text-right">{stat.count}</span>
                    </div>
                  </div>
                ))}
              </div>
            </section>
          </>
        )}
      </div>
    </div>
  );
}
