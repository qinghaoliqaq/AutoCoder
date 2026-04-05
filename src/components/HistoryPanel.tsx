import { SessionMeta } from '../types';
import { relativeTime, getGroup } from '../utils';
import { VscAdd, VscHistory } from 'react-icons/vsc';

interface HistoryPanelProps {
  sessions: SessionMeta[];
  currentSessionId: string;
  onLoad: (sessionId: string) => void;
  onDelete: (sessionId: string) => void;
  onNewChat: () => void;
  onClose: () => void;
}


export default function HistoryPanel({
  sessions,
  currentSessionId,
  onLoad,
  onDelete,
  onNewChat,
  onClose,
}: HistoryPanelProps) {
  const sortedSessions = [...sessions].sort((a, b) => {
    if (b.updated_at !== a.updated_at) return b.updated_at - a.updated_at;
    if (b.created_at !== a.created_at) return b.created_at - a.created_at;
    return a.id.localeCompare(b.id);
  });

  // Group sessions while preserving sort order (most-recent-first)
  const grouped: { label: string; items: SessionMeta[] }[] = [];
  for (const session of sortedSessions) {
    const label = getGroup(session.updated_at);
    const last = grouped[grouped.length - 1];
    if (last && last.label === label) {
      last.items.push(session);
    } else {
      grouped.push({ label, items: [session] });
    }
  }

  return (
    <div className="flex flex-col h-full w-full overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-5 py-4 border-b
                      border-zinc-200/40 dark:border-zinc-700/40 flex-shrink-0 min-h-[52px]">
        <div className="flex items-center gap-2.5">
          <span className="text-[11px] font-bold uppercase tracking-widest text-zinc-800 dark:text-zinc-200 select-none">
            History
          </span>
          {sessions.length > 0 && (
            <span className="rounded-full bg-zinc-200/80 px-1.5 py-0.5 text-[10px] font-semibold tabular-nums text-zinc-600 dark:bg-zinc-800/80 dark:text-zinc-400">
              {sessions.length}
            </span>
          )}
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={onNewChat}
            className="flex h-7 items-center gap-1 rounded-lg bg-violet-500/10 px-2.5 text-[11px] font-semibold text-violet-600 transition-colors hover:bg-violet-500/20 dark:text-violet-400 dark:hover:bg-violet-500/25"
            title="New session"
          >
            <VscAdd className="h-3 w-3" />
            <span>New</span>
          </button>
          <button
            onClick={onClose}
            className="flex h-6 w-6 items-center justify-center rounded-md text-zinc-400
                       transition-colors hover:bg-zinc-200/50 hover:text-zinc-600
                       dark:hover:bg-zinc-800/50 dark:hover:text-zinc-300"
          >
            <svg className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>

      {/* Session list */}
      <div className="flex-1 overflow-y-auto custom-scrollbar">
        {sessions.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full px-6 text-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl border border-zinc-200/60 bg-zinc-100/60 dark:border-zinc-800 dark:bg-zinc-900/60">
              <VscHistory className="h-5 w-5 text-zinc-400 dark:text-zinc-600" />
            </div>
            <div>
              <p className="text-[11px] font-medium text-zinc-500 dark:text-zinc-500">No sessions yet</p>
              <p className="mt-0.5 text-[10px] text-zinc-400 dark:text-zinc-600">
                Conversations are saved automatically
              </p>
            </div>
          </div>
        ) : (
          <div className="px-2 py-2 space-y-3">
            {grouped.map(group => (
              <div key={group.label}>
                {/* Group label */}
                <p className="px-2 pb-1.5 pt-1 text-[10px] font-bold uppercase tracking-[0.1em] text-zinc-400 dark:text-zinc-600 select-none">
                  {group.label}
                </p>

                <div className="space-y-1">
                  {group.items.map(session => (
                    <div key={session.id} className="group relative">
                      <button
                        onClick={() => onLoad(session.id)}
                        className={`w-full rounded-xl px-3 py-2.5 text-left transition-all duration-200 ${
                          session.id === currentSessionId
                            ? 'border border-violet-200/60 bg-violet-50/70 shadow-sm dark:border-violet-500/20 dark:bg-violet-500/10'
                            : 'border border-transparent hover:border-zinc-200/50 hover:bg-white/50 dark:hover:border-zinc-800/50 dark:hover:bg-zinc-900/40'
                        }`}
                      >
                        <div className="flex items-start gap-2.5 pr-5">
                          <div className={`mt-1 flex h-2 w-2 shrink-0 rounded-full ${
                            session.id === currentSessionId
                              ? 'bg-violet-500 shadow-[0_0_6px_rgba(139,92,246,0.5)]'
                              : 'bg-zinc-300 dark:bg-zinc-700'
                          }`} />
                          <div className="min-w-0 flex-1">
                            <p className={`truncate text-[12px] font-medium leading-snug ${
                              session.id === currentSessionId
                                ? 'text-violet-700 dark:text-violet-300'
                                : 'text-zinc-700 dark:text-zinc-300'
                            }`}>
                              {session.title || 'New Session'}
                            </p>
                            <div className="mt-1 flex items-center gap-1.5 text-[10px] text-zinc-400 dark:text-zinc-600">
                              <span className="tabular-nums">{session.message_count} msgs</span>
                              <span className="text-zinc-300 dark:text-zinc-700">|</span>
                              <span>{relativeTime(session.updated_at)}</span>
                            </div>
                          </div>
                        </div>
                      </button>

                      {/* Delete button */}
                      <button
                        onClick={e => { e.stopPropagation(); onDelete(session.id); }}
                        className="absolute right-2 top-1/2 -translate-y-1/2 flex h-6 w-6 items-center justify-center rounded-md
                                   text-zinc-300 opacity-0 transition-all
                                   hover:bg-rose-50 hover:text-rose-500
                                   dark:text-zinc-700 dark:hover:bg-rose-500/10 dark:hover:text-rose-400
                                   group-hover:opacity-100"
                        title="Delete"
                      >
                        <svg className="h-3 w-3" fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                        </svg>
                      </button>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
