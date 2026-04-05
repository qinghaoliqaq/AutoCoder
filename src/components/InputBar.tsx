import { useState, useRef, KeyboardEvent } from 'react';
import { AppMode, SystemStatus, ConfigStatus, MODES } from '../types';
import AccessModeToggle from './AccessModeToggle';

interface InputBarProps {
  mode: AppMode;
  status: SystemStatus | null;
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
    qa: 'QA acceptance in progress — Director is judging readiness from the collected evidence...',
  };

  return (
    <div className="flex flex-col gap-2 relative z-50">
      <div className={`relative flex flex-col gap-1.5 p-1.5 sm:p-2 glass-panel transition-all rounded-3xl
                      ${directorReady ? 'focus-within:border-violet-400/50 focus-within:ring-4 focus-within:ring-violet-500/10 dark:focus-within:border-violet-500/50 dark:focus-within:ring-violet-500/10 shadow-[0_4px_30px_rgba(0,0,0,0.03)] dark:shadow-[0_4px_30px_rgba(0,0,0,0.2)]'
          : 'border-rose-300/50 dark:border-rose-900/50 bg-rose-50/40 dark:bg-rose-900/10 shadow-none'}`}>

        {mode !== 'chat' && (
          <div className="flex flex-wrap items-start justify-between gap-1.5 px-1">
            <div className="self-start flex items-center gap-1.5 px-2.5 py-1.5 glass-button rounded-lg">
              <span className="text-[10px] text-zinc-500 dark:text-zinc-400 uppercase tracking-wider font-semibold">skill</span>
              <span className="text-[11px] font-bold text-violet-600 dark:text-violet-400 flex items-center gap-1.5">
                {MODES.find(m => m.id === mode)?.icon} {mode.toUpperCase()}
              </span>
              <span className="text-[10px] text-zinc-400 dark:text-zinc-500 italic ml-1">by Director</span>
            </div>
          </div>
        )}

        <div className="flex items-end px-1 relative">
          <div className="flex-1 relative">
            <textarea
              ref={textareaRef}
              value={input}
              onChange={(e) => {
                setInput(e.target.value);
                handleInput();
              }}
              onCompositionStart={() => {
                isComposingRef.current = true;
              }}
              onCompositionEnd={() => {
                isComposingRef.current = false;
                lastCompositionEndAtRef.current = Date.now();
              }}
              onKeyDown={handleKeyDown}
              placeholder={directorReady ? placeholders[mode] : 'Use the model settings button in the top bar to configure Director...'}
              disabled={!directorReady || isRunning}
              rows={1}
              className="custom-scrollbar w-full resize-none border-none bg-transparent py-2 text-[14px] leading-6 text-zinc-900 placeholder-zinc-400 transition-all
                       dark:text-zinc-100 dark:placeholder-zinc-500 sm:text-[15px]
                       focus:ring-0 focus:outline-none disabled:opacity-60 disabled:cursor-not-allowed
                       "
              style={{ minHeight: '32px', maxHeight: '400px' }}
            />
          </div>

          <div className="flex-shrink-0 flex items-center justify-center pl-3 pb-0.5">
            {isRunning ? (
              <button
                onClick={onStop}
                disabled={isStopping}
                className="w-10 h-10 flex items-center justify-center bg-rose-50/80 dark:bg-rose-500/10 hover:bg-rose-100 dark:hover:bg-rose-500/20 border border-rose-200/50 dark:border-rose-500/30 rounded-full text-rose-600 dark:text-rose-400 transition-all shadow-sm active:scale-95 backdrop-blur-sm"
                title={isStopping ? 'Stopping...' : 'Stop'}
              >
                <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 16 16">
                  <rect x="3" y="3" width="10" height="10" rx="1.5" />
                </svg>
              </button>
            ) : (
              <button
                onClick={handleSubmit}
                disabled={!input.trim() || !directorReady}
                className={`w-10 h-10 flex items-center justify-center rounded-full transition-all active:scale-95 backdrop-blur-sm
                        ${(!input.trim() || !directorReady) ? 'bg-zinc-100/50 dark:bg-zinc-800/50 text-zinc-400 dark:text-zinc-600 border border-zinc-200/50 dark:border-zinc-700/50' : 'bg-zinc-800 dark:bg-zinc-100 text-white dark:text-zinc-900 shadow-md hover:bg-zinc-900 dark:hover:bg-white'}
                        disabled:opacity-50 disabled:cursor-not-allowed`}
                title="Send (Enter)"
              >
                <svg className="w-5 h-5 ml-[2px]" fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" d="M5 12h14M12 5l7 7-7 7" />
                </svg>
              </button>
            )}
          </div>
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
        <div className="text-[10px] text-zinc-400 dark:text-zinc-500 flex items-center gap-1.5 opacity-70">
          <span><kbd className="font-sans">↵</kbd> Send</span>
          <span>·</span>
          <span><kbd className="font-sans">⇧</kbd><kbd className="font-sans ml-0.5">↵</kbd> New Line</span>
        </div>
      </div>
    </div>
  );
}
