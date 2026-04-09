import { CheckCircle2, Circle, Loader2 } from 'lucide-react';

export interface TodoItem {
  content: string;
  status: 'pending' | 'in_progress' | 'completed';
}

interface Props {
  items: TodoItem[];
}

export default function InlineTodoList({ items }: Props) {
  if (items.length === 0) return null;

  const done = items.filter(t => t.status === 'completed').length;
  const total = items.length;
  const pct = total > 0 ? Math.round((done / total) * 100) : 0;

  return (
    <div className="my-2 ml-[34px] rounded-xl border border-edge-primary/50 bg-surface-secondary/30 overflow-hidden">
      {/* Header with progress */}
      <div className="flex items-center gap-3 px-3.5 py-2 border-b border-edge-primary/30">
        <span className="text-[11px] font-semibold text-content-primary">Tasks</span>
        <span className="text-[10px] tabular-nums text-content-tertiary">{done}/{total}</span>
        <div className="flex-1 h-1.5 rounded-full bg-surface-tertiary/60 overflow-hidden">
          <div
            className="h-full rounded-full transition-all duration-500 ease-out"
            style={{
              width: `${pct}%`,
              backgroundColor: pct === 100 ? 'rgb(16 185 129)' : 'rgb(var(--accent))',
            }}
          />
        </div>
        <span className="text-[10px] tabular-nums text-content-tertiary font-medium">{pct}%</span>
      </div>

      {/* Todo items */}
      <div className="px-3.5 py-2 space-y-1">
        {items.map((item, i) => (
          <div key={i} className="flex items-start gap-2 py-0.5">
            {item.status === 'completed' ? (
              <CheckCircle2 className="h-3.5 w-3.5 flex-shrink-0 text-emerald-500 mt-0.5" />
            ) : item.status === 'in_progress' ? (
              <Loader2 className="h-3.5 w-3.5 flex-shrink-0 text-themed-accent-text mt-0.5 animate-spin" />
            ) : (
              <Circle className="h-3.5 w-3.5 flex-shrink-0 text-content-tertiary mt-0.5" />
            )}
            <span className={`text-[11.5px] leading-[1.5] ${
              item.status === 'completed'
                ? 'text-content-tertiary line-through'
                : item.status === 'in_progress'
                  ? 'text-content-primary font-medium'
                  : 'text-content-secondary'
            }`}>
              {item.status === 'in_progress' ? item.content : item.content}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
