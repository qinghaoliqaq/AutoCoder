import { useState, useRef, KeyboardEvent } from 'react';
import { AppMode, SystemStatus, MODES } from '../types';

interface InputBarProps {
  mode: AppMode;
  status: SystemStatus | null;
  isRunning: boolean;
  onSubmit: (text: string) => void;
  onStop: () => void;
}

export default function InputBar({ mode, status, isRunning, onSubmit, onStop }: InputBarProps) {
  const [input, setInput] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const canRun = status?.claude.installed || status?.codex.installed;

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  };

  const handleSubmit = () => {
    const text = input.trim();
    if (!text || isRunning || !canRun) return;
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
      <div className={`relative flex flex-col p-2 sm:p-2.5 bg-white/80 dark:bg-zinc-900/80 backdrop-blur-xl border 
                      rounded-2xl shadow-lg transition-all gap-1 
                      ${canRun ? 'border-zinc-200/50 dark:border-zinc-700/50 shadow-black/5 dark:shadow-black/20 focus-within:border-violet-400/50 focus-within:ring-4 focus-within:ring-violet-500/10 dark:focus-within:border-violet-500/50 dark:focus-within:ring-violet-500/10'
          : 'border-red-200/50 dark:border-red-900/50'}`}>

        {/* Read-only mode indicator — set by Director only */}
        {mode !== 'chat' && (
          <div className="self-start flex items-center gap-1.5 px-2.5 py-1
                          bg-zinc-100/60 dark:bg-zinc-800/40 rounded-lg
                          border border-zinc-200/50 dark:border-zinc-700/50">
            <span className="text-[10px] text-zinc-400 dark:text-zinc-500">skill</span>
            <span className="text-xs font-semibold text-violet-600 dark:text-violet-400">
              {MODES.find(m => m.id === mode)?.icon} {mode.toUpperCase()}
            </span>
            <span className="text-[10px] text-zinc-400 dark:text-zinc-500 italic">by Director</span>
          </div>
        )}

        <div className="flex items-end mt-1">
          <div className="flex-1 relative">
            <textarea
              ref={textareaRef}
              value={input}
              onChange={(e) => {
                setInput(e.target.value);
                handleInput();
              }}
              onKeyDown={handleKeyDown}
              placeholder={canRun ? placeholders[mode] : 'Please install Claude Code or Codex in your terminal to continue...'}
              disabled={!canRun || isRunning}
              rows={1}
              className="w-full bg-transparent border-none px-3 py-2 text-[15px] sm:text-[16px] text-zinc-900 dark:text-zinc-100 placeholder-zinc-400 dark:placeholder-zinc-500 resize-none custom-scrollbar
                       focus:ring-0 focus:outline-none disabled:opacity-60 disabled:cursor-not-allowed
                       transition-all leading-relaxed"
              style={{ minHeight: '28px', maxHeight: '400px' }}
            />
          </div>

          <div className="flex-shrink-0 flex items-center justify-center pl-2">
            {isRunning ? (
              <button
                onClick={onStop}
                className="w-10 h-10 flex items-center justify-center bg-red-50 dark:bg-red-500/10 hover:bg-red-100 dark:hover:bg-red-500/20 border border-red-200 dark:border-red-500/30 rounded-xl text-red-600 dark:text-red-400 transition-all shadow-sm active:scale-95"
                title="Stop"
              >
                <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 16 16">
                  <rect x="3" y="3" width="10" height="10" rx="1.5" />
                </svg>
              </button>
            ) : (
              <button
                onClick={handleSubmit}
                disabled={!input.trim() || !canRun}
                className={`w-10 h-10 flex items-center justify-center rounded-xl transition-all active:scale-95
                        ${(!input.trim() || !canRun) ? 'bg-zinc-100 dark:bg-zinc-800 text-zinc-400 dark:text-zinc-600' : 'bg-zinc-900 dark:bg-zinc-100 text-white dark:text-zinc-900 hover:bg-zinc-800 dark:hover:bg-white shadow-sm'}
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
          <span className="opacity-75">AI Dev Hub Context Ready</span>
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
