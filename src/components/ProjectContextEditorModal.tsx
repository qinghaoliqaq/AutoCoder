/**
 * ProjectContextEditorModal — modal for pasting / editing project documentation
 * that skills use as context during execution.
 */

interface Props {
  draft: string;
  onChange: (draft: string) => void;
  onSave: (draft: string) => void;
  onClear: () => void;
  onClose: () => void;
}

export default function ProjectContextEditorModal({ draft, onChange, onSave, onClear, onClose }: Props) {
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-white/20 backdrop-blur-xl dark:bg-zinc-950/60"
      onClick={onClose}
      onKeyDown={(e) => { if (e.key === 'Escape') onClose(); }}
    >
      <div
        className="mx-4 flex max-h-[80vh] w-full max-w-2xl flex-col overflow-hidden glass-panel"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-4 border-b border-zinc-200/40 bg-white/10 px-5 py-4 dark:border-zinc-800/50 dark:bg-zinc-900/25">
          <div>
            <h2 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200">Project Context</h2>
            <p className="text-xs text-zinc-500 mt-0.5">粘贴开发文档，Claude & Codex 编码时将以此为依据</p>
          </div>
          <button
            onClick={onClose}
            className="rounded-lg p-1 text-zinc-400 transition-colors hover:bg-zinc-100 hover:text-zinc-600 dark:hover:bg-zinc-800 dark:hover:text-zinc-300"
            title="关闭 (Esc)"
          >
            <svg className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <textarea
          value={draft}
          onChange={(e) => onChange(e.target.value)}
          placeholder="粘贴你的开发文档、需求说明、技术规范..."
          className="flex-1 bg-transparent p-4 text-sm font-mono text-zinc-800 dark:text-zinc-200
                     border-none resize-none focus:outline-none min-h-[300px]
                     placeholder-zinc-400 dark:placeholder-zinc-600"
          autoFocus
        />
        <div className="flex items-center justify-between border-t border-zinc-200/40 bg-white/10 px-5 py-3 dark:border-zinc-800/50 dark:bg-zinc-900/25">
          <button
            onClick={onClear}
            className="text-xs text-rose-500 hover:text-rose-600 transition-colors font-medium"
          >
            清除文档
          </button>
          <div className="flex gap-2">
            <button
              onClick={onClose}
              className="text-xs px-4 py-1.5 rounded-lg glass-button text-zinc-600 dark:text-zinc-300 font-medium"
            >
              取消
            </button>
            <button
              onClick={() => onSave(draft)}
              disabled={!draft.trim()}
              className="text-xs px-4 py-1.5 rounded-lg bg-violet-600/90 text-white shadow-md shadow-violet-500/20 backdrop-blur-sm
                         hover:bg-violet-600 hover:shadow-lg hover:shadow-violet-500/30 font-medium disabled:opacity-50 disabled:cursor-not-allowed transition-all"
            >
              应用文档
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
