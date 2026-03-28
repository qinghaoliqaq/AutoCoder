import { useState, useRef, KeyboardEvent } from 'react';
import { AppMode, SystemStatus, ConfigStatus, MODES } from '../types';

interface InputBarProps {
  mode: AppMode;
  status: SystemStatus | null;
  configStatus: ConfigStatus | null;
  isRunning: boolean;
  isStopping: boolean;
  onSubmit: (text: string) => void;
  onStop: () => void;
}

export default function InputBar({ mode, status, configStatus, isRunning, isStopping, onSubmit, onStop }: InputBarProps) {
  const [input, setInput] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const directorReady = configStatus?.configured ?? false;
  const toolReady = status?.claude.installed || status?.codex.installed;

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
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
  };

  return (
    <div className="flex flex-col gap-2">
      <div className={`relative flex flex-col p-2 sm:p-2.5 glass-panel transition-all gap-1 
                      ${directorReady ? 'focus-within:border-violet-400/50 focus-within:ring-4 focus-within:ring-violet-500/10 dark:focus-within:border-violet-500/50 dark:focus-within:ring-violet-500/10'
          : 'border-rose-300/50 dark:border-rose-900/50 bg-rose-50/40 dark:bg-rose-900/10'}`}>

        {/* Read-only mode indicator — set by Director only */}
        {mode !== 'chat' && (
          <div className="self-start flex items-center gap-1.5 px-3 py-1.5
                          glass-button rounded-lg">
            <span className="text-[10px] text-zinc-500 dark:text-zinc-400 uppercase tracking-wider font-semibold">skill</span>
            <span className="text-xs font-bold text-violet-600 dark:text-violet-400 flex items-center gap-1.5">
              {MODES.find(m => m.id === mode)?.icon} {mode.toUpperCase()}
            </span>
            <span className="text-[10px] text-zinc-400 dark:text-zinc-500 italic ml-1">by Director</span>
          </div>
        )}

        <div className="flex items-end mt-1 px-1 relative">
          <div className="flex-1 relative">
            <textarea
              ref={textareaRef}
              value={input}
              onChange={(e) => {
                setInput(e.target.value);
                handleInput();
              }}
              onKeyDown={handleKeyDown}
              placeholder={directorReady ? placeholders[mode] : 'Configure Director in config.toml or environment variables to continue...'}
              disabled={!directorReady || isRunning}
              rows={1}
              className="w-full bg-transparent border-none py-2 text-[15px] sm:text-[16px] text-zinc-900 dark:text-zinc-100 placeholder-zinc-400 dark:placeholder-zinc-500 resize-none custom-scrollbar
                       focus:ring-0 focus:outline-none disabled:opacity-60 disabled:cursor-not-allowed
                       transition-all leading-relaxed"
              style={{ minHeight: '32px', maxHeight: '400px' }}
            />
          </div>

          <div className="flex-shrink-0 flex items-center justify-center pl-3 pb-1">
            {isRunning ? (
              <button
                onClick={onStop}
                disabled={isStopping}
                className="w-10 h-10 flex items-center justify-center bg-rose-50/80 dark:bg-rose-500/10 hover:bg-rose-100 dark:hover:bg-rose-500/20 border border-rose-200/50 dark:border-rose-500/30 rounded-xl text-rose-600 dark:text-rose-400 transition-all shadow-sm active:scale-95 backdrop-blur-sm"
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
                className={`w-10 h-10 flex items-center justify-center rounded-xl transition-all active:scale-95 backdrop-blur-sm
                        ${(!input.trim() || !directorReady) ? 'bg-zinc-100/50 dark:bg-zinc-800/50 text-zinc-400 dark:text-zinc-600 border border-zinc-200/50 dark:border-zinc-700/50' : 'bg-violet-600/90 text-white shadow-md shadow-violet-500/20 hover:bg-violet-600 hover:shadow-lg hover:shadow-violet-500/30'}
                        disabled:opacity-50 disabled:cursor-not-allowed`}
                title="Send (Enter)"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" strokeWidth={2.5} viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8" />
                </svg>
              </button>
            )}
          </div>
        </div>
      </div>

      <div className="flex items-center justify-between px-2">
        <div className="text-[10px] text-zinc-400 dark:text-zinc-500 font-medium">
          <span className="opacity-75">
            {!directorReady
              ? 'Director not configured'
              : toolReady
                ? 'Director and local tools ready'
                : 'Director ready; Claude/Codex skills may fail until local CLIs are installed'}
          </span>
        </div>
        <div className="text-[10px] text-zinc-400 dark:text-zinc-500 flex items-center gap-1.5 opacity-70">
          <span><kbd className="font-sans">↵</kbd> Send</span>
          <span>·</span>
          <span><kbd className="font-sans">⇧</kbd><kbd className="font-sans ml-0.5">↵</kbd> New Line</span>
        </div>
      </div>
    </div>
  );
}
