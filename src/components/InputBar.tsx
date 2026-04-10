import { useState, useRef, KeyboardEvent } from 'react';
import { AppMode, ConfigStatus, MODES } from '../types';
import AccessModeToggle from './AccessModeToggle';

interface InputBarProps {
  mode: AppMode;
  configStatus: ConfigStatus | null;
  isRunning: boolean;
  isStopping: boolean;
  configUpdating?: boolean;
  onSubmit: (text: string) => void;
  onStop: () => void;
  onToggleExecutionAccess: (mode: ConfigStatus['execution_access_mode']) => void;
}

export default function InputBar({
  mode,
  configStatus,
  isRunning,
  isStopping,
  configUpdating = false,
  onSubmit,
  onStop,
  onToggleExecutionAccess,
}: InputBarProps) {
  const [input, setInput] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const isComposingRef = useRef(false);
  const lastCompositionEndAtRef = useRef(0);

  const directorReady = configStatus?.configured ?? false;

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    const nativeEvent = e.nativeEvent as KeyboardEvent['nativeEvent'] & { keyCode?: number };
    const compositionJustEnded = Date.now() - lastCompositionEndAtRef.current < 40;

    if (
      e.nativeEvent.isComposing ||
      isComposingRef.current ||
      nativeEvent.keyCode === 229 ||
      compositionJustEnded
    ) {
      return;
    }

    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  };

  const handleSubmit = () => {
    const text = input.trim();
    if (!text || isRunning || !directorReady) return;
    onSubmit(text);
    setInput('');
    // Reset textarea height
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
    }
  };

  const handleInput = () => {
    const el = textareaRef.current;
    if (el) {
      el.style.height = 'auto';
      el.style.height = Math.min(el.scrollHeight, 160) + 'px';
    }
  };

  const placeholders: Record<AppMode, string> = {
    chat: 'Talk to Director...',
    plan: 'Describe your requirements, Director will coordinate Claude and Codex...',
    code: 'Tell Claude what features to implement...',
    debug: 'Describe the issue, Codex will help debug...',
    test: 'Tell Claude what module or scenario to test...',
    review: 'Code Review in progress — Director is auditing, cleaning and testing...',
    document: 'Generating project completion report (PROJECT_REPORT.md)...',
  };

  const hasInput = input.trim().length > 0;

  return (
    <div className="flex flex-col gap-1.5 relative z-50">
      <div
        className={`relative flex flex-col rounded-2xl border transition-all duration-200 ${
          directorReady
            ? 'border-edge-primary/40 bg-surface-secondary/50 backdrop-blur-2xl shadow-[0_2px_20px_rgb(var(--bg-primary)/0.08)] focus-within:border-themed-accent/40 focus-within:shadow-[0_2px_24px_rgb(var(--accent)/0.1)]'
            : 'border-rose-400/30 bg-rose-50/20'
        }`}
      >
        {mode !== 'chat' && (
          <div className="flex items-center gap-1.5 px-4 pt-2.5">
            <span className="text-[10px] text-content-tertiary uppercase tracking-wider font-semibold">skill</span>
            <span className="text-[11px] font-bold text-themed-accent-text flex items-center gap-1">
              {MODES.find(m => m.id === mode)?.icon} {mode.toUpperCase()}
            </span>
          </div>
        )}

        <div className="flex items-end gap-2 px-4 py-2.5">
          <textarea
            ref={textareaRef}
            value={input}
            onChange={(e) => {
              setInput(e.target.value);
              handleInput();
            }}
            onCompositionStart={() => { isComposingRef.current = true; }}
            onCompositionEnd={() => {
              isComposingRef.current = false;
              lastCompositionEndAtRef.current = Date.now();
            }}
            onKeyDown={handleKeyDown}
            placeholder={directorReady ? placeholders[mode] : 'Use the model settings button in the top bar to configure Director...'}
            disabled={!directorReady || isRunning}
            rows={1}
            className="custom-scrollbar flex-1 resize-none border-none bg-transparent text-[14px] leading-6 text-content-primary placeholder-content-tertiary
                     sm:text-[15px] focus:ring-0 focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed"
            style={{ minHeight: '28px', maxHeight: '400px' }}
          />

          {isRunning ? (
            <button
              onClick={onStop}
              disabled={isStopping}
              className="flex-shrink-0 w-8 h-8 flex items-center justify-center rounded-lg text-rose-500 border border-rose-300/30 transition-all active:scale-95 hover:bg-rose-500/10"
              title={isStopping ? 'Stopping...' : 'Stop'}
            >
              <svg className="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 16 16">
                <rect x="3" y="3" width="10" height="10" rx="1.5" />
              </svg>
            </button>
          ) : (
            <button
              onClick={handleSubmit}
              disabled={!hasInput || !directorReady}
              className={`flex-shrink-0 w-8 h-8 flex items-center justify-center rounded-lg transition-all active:scale-95 ${
                hasInput && directorReady
                  ? 'bg-themed-accent text-white shadow-sm hover:opacity-90'
                  : 'text-content-tertiary/40'
              } disabled:cursor-not-allowed`}
              title="Send (Enter)"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" strokeWidth={2.5} viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 10.5L12 3m0 0l7.5 7.5M12 3v18" />
              </svg>
            </button>
          )}
        </div>
      </div>

      <div className="flex items-center justify-between gap-3 px-2">
        {configStatus ? (
          <AccessModeToggle
            mode={configStatus.execution_access_mode}
            compact
            disabled={!directorReady || isRunning || configUpdating}
            onChange={onToggleExecutionAccess}
          />
        ) : (
          <div />
        )}
        <div className="text-[10px] text-content-tertiary/60 flex items-center gap-1.5 select-none">
          <kbd className="rounded border border-edge-primary/40 px-1 py-0.5 font-mono text-[9px]">↵</kbd>
          <span>发送</span>
          <span className="mx-0.5">·</span>
          <kbd className="rounded border border-edge-primary/40 px-1 py-0.5 font-mono text-[9px]">⇧↵</kbd>
          <span>换行</span>
        </div>
      </div>
    </div>
  );
}
