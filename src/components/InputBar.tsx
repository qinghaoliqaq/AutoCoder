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
    qa: 'QA acceptance in progress — Director is judging readiness from the collected evidence...',
  };

  return (
    <div className="flex flex-col gap-2 relative z-50">
      <div className={`relative flex flex-col gap-1.5 p-1.5 sm:p-2 glass-panel transition-all rounded-3xl
                      ${directorReady ? 'focus-within:border-themed-accent/40 focus-within:ring-4 focus-within:ring-themed-accent/10 shadow-[0_4px_30px_rgb(var(--bg-primary)/0.1)]'
          : 'border-rose-400/40 bg-rose-50/30 shadow-none'}`}
        style={!directorReady ? { backgroundColor: 'rgb(var(--bg-primary) / 0.5)' } : undefined}
      >

        {mode !== 'chat' && (
          <div className="flex flex-wrap items-start justify-between gap-1.5 px-1">
            <div className="self-start flex items-center gap-1.5 px-2.5 py-1.5 glass-button rounded-lg">
              <span className="text-[10px] text-content-tertiary uppercase tracking-wider font-semibold">skill</span>
              <span className="text-[11px] font-bold text-themed-accent-text flex items-center gap-1.5">
                {MODES.find(m => m.id === mode)?.icon} {mode.toUpperCase()}
              </span>
              <span className="text-[10px] text-content-tertiary italic ml-1">by Director</span>
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
              className="custom-scrollbar w-full resize-none border-none bg-transparent py-2 text-[14px] leading-6 text-content-primary placeholder-content-tertiary transition-all
                       sm:text-[15px]
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
                className="w-10 h-10 flex items-center justify-center rounded-full text-rose-500 transition-all shadow-sm active:scale-95 backdrop-blur-sm border border-rose-300/40"
                style={{ backgroundColor: 'rgb(var(--bg-elevated) / 0.8)' }}
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
                        ${(!input.trim() || !directorReady) ? 'text-content-tertiary border border-edge-primary/50' : 'bg-themed-accent text-white shadow-md hover:opacity-90'}
                        disabled:opacity-50 disabled:cursor-not-allowed`}
                style={(!input.trim() || !directorReady) ? { backgroundColor: 'rgb(var(--bg-elevated) / 0.5)' } : undefined}
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

      <div className="flex items-center justify-between gap-3 px-3">
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
        <div className="text-[10px] text-content-tertiary flex items-center gap-2 tabular-nums select-none">
          <kbd className="rounded border border-edge-primary/60 bg-surface-tertiary/60 px-1 py-0.5 font-mono text-[9px]">Enter</kbd>
          <span>Send</span>
          <span className="text-edge-primary">|</span>
          <kbd className="rounded border border-edge-primary/60 bg-surface-tertiary/60 px-1 py-0.5 font-mono text-[9px]">Shift+Enter</kbd>
          <span>Newline</span>
        </div>
      </div>
    </div>
  );
}
