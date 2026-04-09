import { useState } from 'react';
import { Terminal, FileText, FilePen, Pencil, Search, SearchCode, Sparkles, CheckCircle2, Wrench, ChevronRight } from 'lucide-react';
import { ToolLog } from '../types';

const TOOL_META: Record<string, { icon: React.ReactNode; color: string; bg: string }> = {
  Shell:    { icon: <Terminal className="h-3 w-3" />, color: 'text-emerald-500', bg: 'bg-emerald-500/10' },
  Bash:     { icon: <Terminal className="h-3 w-3" />, color: 'text-emerald-500', bg: 'bg-emerald-500/10' },
  bash:     { icon: <Terminal className="h-3 w-3" />, color: 'text-emerald-500', bg: 'bg-emerald-500/10' },
  Read:     { icon: <FileText className="h-3 w-3" />, color: 'text-sky-500', bg: 'bg-sky-500/10' },
  Write:    { icon: <FilePen className="h-3 w-3" />, color: 'text-amber-500', bg: 'bg-amber-500/10' },
  Edit:     { icon: <Pencil className="h-3 w-3" />, color: 'text-amber-500', bg: 'bg-amber-500/10' },
  Glob:     { icon: <Search className="h-3 w-3" />, color: 'text-pink-500', bg: 'bg-pink-500/10' },
  Grep:     { icon: <SearchCode className="h-3 w-3" />, color: 'text-pink-500', bg: 'bg-pink-500/10' },
  BundledSkill: { icon: <Sparkles className="h-3 w-3" />, color: 'text-themed-accent-text', bg: 'bg-themed-accent-soft/30' },
  StructuredAcceptance: { icon: <CheckCircle2 className="h-3 w-3" />, color: 'text-themed-accent-text', bg: 'bg-themed-accent-soft/30' },
};

const DEFAULT_META = { icon: <Wrench className="h-3 w-3" />, color: 'text-content-tertiary', bg: 'bg-surface-tertiary/50' };

/** Summarise tool input for the collapsed one-liner. */
function summariseInput(tool: string, input: string): string {
  if (!input) return '';
  // For file ops, show just the path
  if (tool === 'Read' || tool === 'Write' || tool === 'Edit') {
    const match = input.match(/"file_path"\s*:\s*"([^"]+)"/);
    if (match) return match[1].split('/').slice(-2).join('/');
  }
  // For Bash, show the command
  if (tool === 'Bash' || tool === 'bash' || tool === 'Shell') {
    const match = input.match(/"command"\s*:\s*"([^"]+)"/);
    if (match) return match[1].length > 60 ? match[1].slice(0, 60) + '…' : match[1];
  }
  // For Grep/Glob, show the pattern
  if (tool === 'Grep' || tool === 'Glob') {
    const match = input.match(/"pattern"\s*:\s*"([^"]+)"/);
    if (match) return match[1];
  }
  return input.length > 60 ? input.slice(0, 60) + '…' : input;
}

interface InlineToolCallProps {
  log: ToolLog;
}

export default function InlineToolCall({ log }: InlineToolCallProps) {
  const [expanded, setExpanded] = useState(false);
  const meta = TOOL_META[log.tool] ?? DEFAULT_META;
  const summary = summariseInput(log.tool, log.input);

  return (
    <div className="my-1 ml-[34px]">
      <button
        onClick={() => setExpanded(v => !v)}
        className={`group flex w-full items-center gap-2 rounded-lg border px-2.5 py-1.5 text-left transition-all duration-150
          border-edge-primary/40 hover:border-edge-primary/60
          ${expanded ? 'bg-surface-secondary/40' : 'bg-surface-secondary/20 hover:bg-surface-secondary/30'}`}
      >
        <ChevronRight className={`h-3 w-3 flex-shrink-0 text-content-tertiary transition-transform duration-150 ${expanded ? 'rotate-90' : ''}`} />
        <span className={`flex items-center ${meta.color}`}>{meta.icon}</span>
        <span className="text-[11px] font-semibold text-shimmer">{log.tool}</span>
        {!expanded && summary && (
          <span className="truncate text-[11px] text-content-tertiary font-mono">{summary}</span>
        )}
        <span className="ml-auto text-[10px] tabular-nums text-content-tertiary">
          {new Date(log.timestamp).toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit', second: '2-digit', hour12: false })}
        </span>
      </button>
      {expanded && log.input && (
        <div className="mt-0.5 ml-5 rounded-lg border border-edge-primary/30 bg-surface-tertiary/30 px-3 py-2">
          <pre className="whitespace-pre-wrap break-all font-mono text-[10.5px] leading-[1.6] text-content-secondary">
            {log.input}
          </pre>
        </div>
      )}
    </div>
  );
}

/** Group of consecutive tool calls, shown as a compact stack. */
export function InlineToolCallGroup({ logs }: { logs: ToolLog[] }) {
  const [expanded, setExpanded] = useState(false);

  if (logs.length === 1) {
    return <InlineToolCall log={logs[0]} />;
  }

  // Aggregate tool counts for collapsed summary
  const toolCounts = new Map<string, number>();
  for (const log of logs) {
    toolCounts.set(log.tool, (toolCounts.get(log.tool) ?? 0) + 1);
  }
  const summaryParts = Array.from(toolCounts.entries()).map(([tool, count]) =>
    count > 1 ? `${tool} ×${count}` : tool
  );

  return (
    <div className="my-1.5 ml-[34px]">
      <button
        onClick={() => setExpanded(v => !v)}
        className={`group flex w-full items-center gap-2 rounded-lg border px-2.5 py-1.5 text-left transition-all duration-150
          border-edge-primary/40 hover:border-edge-primary/60
          ${expanded ? 'bg-surface-secondary/40' : 'bg-surface-secondary/20 hover:bg-surface-secondary/30'}`}
      >
        <ChevronRight className={`h-3 w-3 flex-shrink-0 text-content-tertiary transition-transform duration-150 ${expanded ? 'rotate-90' : ''}`} />
        <Wrench className="h-3 w-3 text-content-tertiary" />
        <span className="text-[11px] font-semibold text-content-primary">{logs.length} tool calls</span>
        <span className="truncate text-[11px] text-content-tertiary">{summaryParts.join(', ')}</span>
      </button>
      {expanded && (
        <div className="mt-1 space-y-0.5">
          {logs.map((log, i) => (
            <InlineToolCall key={i} log={log} />
          ))}
        </div>
      )}
    </div>
  );
}
