/**
 * UserQuestionModal — surfaces an agent's `AskUserQuestion` request
 * and routes the reply back via `useUserQuestion`.
 *
 * Renders the question, optional preset choices as buttons, and a
 * free-form textarea. Either path resolves the same `submit_user_answer`
 * Tauri command. Intentionally non-dismissable on outside click — the
 * agent loop is blocked waiting for this reply, so accidentally
 * dismissing it would leave the agent hanging until its timeout
 * elapsed (default 5 min). The user must either click an option or
 * submit text.
 */

import { useEffect, useRef, useState } from 'react';
import type { PendingQuestion } from '../hooks/useUserQuestion';
import { HelpCircle } from 'lucide-react';

interface Props {
  question: PendingQuestion;
  onSubmit: (answer: string) => void;
}

export default function UserQuestionModal({ question, onSubmit }: Props) {
  const [draft, setDraft] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  // Reset draft + focus textarea whenever the active question changes
  // (the modal is re-used across consecutive questions from the same
  // agent run).
  useEffect(() => {
    setDraft('');
    setSubmitting(false);
    // Defer to next frame so the modal is painted before we focus —
    // otherwise the autofocus race against the entry animation drops
    // the focus on slow renders.
    const t = setTimeout(() => textareaRef.current?.focus(), 0);
    return () => clearTimeout(t);
  }, [question.request_id]);

  const submit = (answer: string) => {
    if (submitting || !answer.trim()) return;
    setSubmitting(true);
    onSubmit(answer.trim());
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // Cmd/Ctrl+Enter submits — matches the chat input's convention.
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      submit(draft);
    }
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="user-question-title"
      className="fixed inset-0 z-50 flex items-center justify-center bg-white/20 backdrop-blur-xl dark:bg-zinc-950/60"
    >
      <div
        className="mx-4 flex max-h-[85vh] w-full max-w-xl flex-col overflow-hidden glass-panel"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start gap-3 border-b border-zinc-200/40 bg-white/10 px-5 py-4 dark:border-zinc-800/50 dark:bg-zinc-900/25">
          <div className="mt-0.5 rounded-full bg-amber-100/70 p-1.5 text-amber-700 dark:bg-amber-900/30 dark:text-amber-300">
            <HelpCircle className="h-4 w-4" />
          </div>
          <div className="min-w-0 flex-1">
            <h2
              id="user-question-title"
              className="text-sm font-semibold text-zinc-800 dark:text-zinc-200"
            >
              Agent needs your input
            </h2>
            <p className="text-xs text-zinc-500 mt-0.5 truncate">
              from <code className="font-mono text-[11px]">{question.agent_id}</code>
            </p>
          </div>
        </div>

        <div className="flex flex-col gap-4 overflow-y-auto px-5 py-4">
          <p className="whitespace-pre-wrap text-sm text-zinc-800 dark:text-zinc-200">
            {question.question}
          </p>

          {question.options.length > 0 && (
            <div className="flex flex-wrap gap-2">
              {question.options.map((opt) => (
                <button
                  key={opt}
                  onClick={() => submit(opt)}
                  disabled={submitting}
                  className="rounded-lg border border-zinc-200/60 bg-white/40 px-3 py-1.5 text-xs font-medium
                             text-zinc-700 transition-colors hover:bg-white/70 hover:text-zinc-900
                             disabled:cursor-not-allowed disabled:opacity-50
                             dark:border-zinc-700/60 dark:bg-zinc-800/40 dark:text-zinc-200
                             dark:hover:bg-zinc-800/70 dark:hover:text-zinc-100"
                >
                  {opt}
                </button>
              ))}
            </div>
          )}

          <div className="flex flex-col gap-1.5">
            <label className="text-[11px] font-medium uppercase tracking-wide text-zinc-500 dark:text-zinc-400">
              {question.options.length > 0 ? 'Or type your own reply' : 'Your reply'}
            </label>
            <textarea
              ref={textareaRef}
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={onKeyDown}
              rows={4}
              placeholder="Type your reply… (Cmd/Ctrl+Enter to submit)"
              disabled={submitting}
              className="w-full resize-none rounded-lg border border-edge-primary/40 bg-surface-input/60 px-3 py-2 text-sm
                         text-zinc-800 outline-none placeholder:text-zinc-400
                         focus:border-themed-accent/50 focus:ring-1 focus:ring-themed-accent/20
                         disabled:cursor-not-allowed disabled:opacity-50
                         dark:text-zinc-200 dark:placeholder:text-zinc-600"
            />
          </div>
        </div>

        <div className="flex items-center justify-end gap-2 border-t border-zinc-200/40 bg-white/10 px-5 py-3 dark:border-zinc-800/50 dark:bg-zinc-900/25">
          <button
            onClick={() => submit(draft)}
            disabled={!draft.trim() || submitting}
            className="rounded-lg bg-violet-600/90 px-4 py-1.5 text-xs font-medium text-white shadow-md shadow-violet-500/20
                       transition-all hover:bg-violet-600 hover:shadow-lg hover:shadow-violet-500/30
                       disabled:cursor-not-allowed disabled:opacity-50"
          >
            {submitting ? 'Sending…' : 'Send reply'}
          </button>
        </div>
      </div>
    </div>
  );
}
