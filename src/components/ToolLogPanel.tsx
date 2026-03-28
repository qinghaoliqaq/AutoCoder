import { useRef, useEffect } from 'react';
import { ToolLog } from '../types';

interface Props {
  logs: ToolLog[];
  onClose: () => void;
}

const AGENT_COLOR: Record<string, string> = {
  claude: 'text-[#cc785c]',
  codex: 'text-[#10a37f]',
};

const TOOL_ICON: Record<string, string> = {
  Shell: '🖥️',
  Bash: '⚡',
  bash: '⚡',
  VendoredSkill: '🧩',
  Read: '📖',
  Write: '✍️',
  Edit: '✏️',
  Glob: '🔍',
  Grep: '🔎',
};

export default function ToolLogPanel({ logs, onClose }: Props) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [logs]);

  return (
    <div
      className="flex flex-col h-full w-full overflow-hidden"
    >
      {/* Header */}
      <div className="flex items-center justify-between px-5 py-4 border-b
                      border-zinc-200/40 dark:border-zinc-700/40 flex-shrink-0 min-h-[52px]">
        <span className="text-[11px] font-bold uppercase tracking-widest text-zinc-800 dark:text-zinc-200 select-none whitespace-nowrap">
          工具日志
        </span>
        <button
          onClick={onClose}
          className="text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300 text-lg leading-none ml-2"
        >
          ×
        </button>
      </div>

      {/* Log entries */}
      <div className="flex-1 overflow-y-auto px-2 py-2 space-y-1.5 min-h-0">
        {logs.length === 0 && (
          <p className="text-xs text-zinc-400 dark:text-zinc-600 text-center mt-6 px-2">
            等待工具调用...
          </p>
        )}
        {logs.map((log, i) => (
          <div
            key={i}
            className="rounded-lg bg-zinc-50 dark:bg-zinc-800/60 px-2.5 py-2
                       border border-zinc-200/60 dark:border-zinc-700/50"
          >
            <div className="flex items-center gap-1.5 mb-0.5 flex-wrap">
              <span className="text-xs">{TOOL_ICON[log.tool] ?? '🛠'}</span>
              <span className={`text - [10px] font - semibold ${AGENT_COLOR[log.agent] ?? 'text-zinc-500'} `}>
                {log.agent}
              </span>
              <span className="text-[11px] font-mono font-bold text-zinc-700 dark:text-zinc-300">
                {log.tool}
              </span>
            </div>
            {log.input && (
              <p className="text-[10px] font-mono text-zinc-500 dark:text-zinc-500
                            break-all leading-relaxed line-clamp-3">
                {log.input}
              </p>
            )}
          </div>
        ))}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
