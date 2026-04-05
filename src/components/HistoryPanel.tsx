import { SessionMeta } from '../types';
import { relativeTime, getGroup } from '../utils';

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
    <div className="flex flex-col h-full text-sm">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-zinc-200 dark:border-zinc-800 flex-shrink-0">
        <span className="font-medium text-zinc-700 dark:text-zinc-300 text-xs uppercase tracking-wider">
          历史对话
        </span>
        <div className="flex items-center gap-1">
          <button
            onClick={onNewChat}
            className="text-xs px-2 py-1 rounded bg-violet-100 dark:bg-violet-500/20 text-violet-600 dark:text-violet-400 hover:bg-violet-200 dark:hover:bg-violet-500/30 transition-colors"
            title="新建对话"
          >
            + 新建
          </button>
          <button
            onClick={onClose}
            className="w-6 h-6 flex items-center justify-center rounded text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300 hover:bg-zinc-100 dark:hover:bg-zinc-800 transition-colors"
            title="关闭"
          >
            ✕
          </button>
        </div>
      </div>

      {/* Session list */}
      <div className="flex-1 overflow-y-auto">
        {sessions.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-zinc-400 dark:text-zinc-600 gap-2 px-6 text-center">
            <span className="text-2xl">💬</span>
            <p className="text-xs">还没有历史对话</p>
            <p className="text-xs">发送第一条消息后自动保存</p>
          </div>
        ) : (
          <div className="py-1">
            {grouped.map(group => (
              <div key={group.label}>
                {/* Group label */}
                <p className="px-4 pt-3 pb-1 text-[10px] font-semibold uppercase tracking-wider text-zinc-400 dark:text-zinc-600 select-none">
                  {group.label}
                </p>

                <ul>
                  {group.items.map(session => (
                    <li key={session.id} className="group relative">
                      <button
                        onClick={() => onLoad(session.id)}
                        className={`w-full text-left px-4 py-2.5 transition-colors ${
                          session.id === currentSessionId
                            ? 'bg-violet-50 dark:bg-violet-500/10 border-l-2 border-violet-500'
                            : 'hover:bg-zinc-50 dark:hover:bg-zinc-800/50 border-l-2 border-transparent'
                        }`}
                      >
                        <div className="flex items-start gap-2 pr-6">
                          <span className={`mt-0.5 text-xs flex-shrink-0 ${
                            session.id === currentSessionId
                              ? 'text-violet-500'
                              : 'text-zinc-300 dark:text-zinc-600'
                          }`}>●</span>
                          <div className="min-w-0">
                            <p className={`truncate text-xs font-medium leading-snug ${
                              session.id === currentSessionId
                                ? 'text-violet-700 dark:text-violet-300'
                                : 'text-zinc-700 dark:text-zinc-300'
                            }`}>
                              {session.title || '新对话'}
                            </p>
                            <p className="text-zinc-400 dark:text-zinc-600 text-[11px] mt-0.5">
                              {session.message_count} 条消息 · {relativeTime(session.updated_at)}
                            </p>
                          </div>
                        </div>
                      </button>

                      {/* Delete button — visible on hover */}
                      <button
                        onClick={e => { e.stopPropagation(); onDelete(session.id); }}
                        className="absolute right-2 top-1/2 -translate-y-1/2 w-6 h-6 flex items-center justify-center rounded text-zinc-300 hover:text-rose-500 dark:text-zinc-700 dark:hover:text-rose-400 hover:bg-rose-50 dark:hover:bg-rose-500/10 opacity-0 group-hover:opacity-100 transition-all"
                        title="删除"
                      >
                        ✕
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
