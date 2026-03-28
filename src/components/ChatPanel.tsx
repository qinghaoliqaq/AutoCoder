import { useEffect, useRef, useState } from 'react';
import { AnimatedMessageIcon, AnimatedFolderIcon, AnimatedSparklesIcon } from './icons/AnimatedIcons';
import { ChatMessage, AgentRole } from '../types';

const ROLE_CONFIG: Record<AgentRole, { label: string; color: string; bg: string }> = {
  claude: {
    label: 'Claude',
    color: 'text-orange-600 dark:text-accent-claude',
    bg: 'bg-orange-100 dark:bg-accent-claude/20 border-orange-200 dark:border-accent-claude/30',
  },
  codex: {
    label: 'Codex',
    color: 'text-emerald-600 dark:text-accent-codex',
    bg: 'bg-emerald-100 dark:bg-accent-codex/20 border-emerald-200 dark:border-accent-codex/30',
  },
  director: {
    label: 'Director',
    color: 'text-violet-600 dark:text-violet-400',
    bg: 'bg-violet-100 dark:bg-violet-500/20 border-violet-200 dark:border-violet-500/30',
  },
  user: {
    label: 'You',
    color: 'text-zinc-700 dark:text-zinc-300',
    bg: 'bg-zinc-200 dark:bg-zinc-700/80 border-zinc-300 dark:border-zinc-600/80',
  },
};

function ReportCard({ content }: { content: string }) {
  const [expanded, setExpanded] = useState(false);
  const title = content.split('\n')[0].replace(/^#+\s*/, '');
  const preview = content.split('\n').slice(1, 6).join('\n').trim();

  return (
    <div className="rounded-xl overflow-hidden border border-violet-200 dark:border-violet-500/30 w-full">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2.5
                      bg-violet-50 dark:bg-violet-500/10 border-b border-violet-200 dark:border-violet-500/20">
        <div className="flex items-center gap-2">
          <span>📄</span>
          <span className="text-xs font-semibold text-violet-700 dark:text-violet-300">{title}</span>
        </div>
        <button
          onClick={() => setExpanded(v => !v)}
          className="text-xs text-violet-600 dark:text-violet-400 hover:underline font-medium"
        >
          {expanded ? 'Collapse' : 'View Report'}
        </button>
      </div>

      {/* Body */}
      {expanded ? (
        <pre className="p-4 text-xs whitespace-pre-wrap font-mono leading-relaxed
                        max-h-[60vh] overflow-y-auto custom-scrollbar
                        text-zinc-700 dark:text-zinc-300 bg-white dark:bg-zinc-900">
          {content}
        </pre>
      ) : (
        <div className="px-4 py-3 bg-white dark:bg-zinc-900">
          <pre className="text-xs whitespace-pre-wrap font-mono text-zinc-400 dark:text-zinc-500 line-clamp-3">
            {preview}
          </pre>
          <button
            onClick={() => setExpanded(true)}
            className="mt-2 text-xs text-violet-500 hover:text-violet-700 dark:hover:text-violet-300 font-medium"
          >
            Click to view full plan →
          </button>
        </div>
      )}
    </div>
  );
}

interface MessageBubbleProps {
  message: ChatMessage;
}

function MessageBubble({ message }: MessageBubbleProps) {
  const config = ROLE_CONFIG[message.role];
  const isUser = message.role === 'user';

  return (
    <div className={`flex gap-3 animate-slide-up ${isUser ? 'flex-row-reverse' : 'flex-row'}`}>
      {/* Avatar */}
      <div className={`flex-shrink-0 w-8 h-8 rounded-full flex items-center justify-center
                       text-sm font-bold border ${config.bg} shadow-sm`}>
        <span className={config.color}>
          {message.role === 'claude' ? 'C' :
            message.role === 'codex' ? 'X' :
              message.role === 'director' ? 'D' : 'U'}
        </span>
      </div>

      <div className={`w-full max-w-4xl space-y-1.5 ${isUser ? 'items-end' : 'items-start'} flex flex-col`}>
        <div className={`flex items-center gap-2 ${isUser ? 'flex-row-reverse' : 'flex-row'}`}>
          <span className={`text-xs font-semibold ${config.color}`}>{config.label}</span>
          <span className="text-[11px] text-zinc-400 dark:text-zinc-500">
            {new Date(message.timestamp).toLocaleTimeString('en-US', {
              hour: 'numeric',
              minute: '2-digit',
              hour12: true,
            })}
          </span>
        </div>

        <div className={`chat-bubble ${message.role}`}>
          {message.thinking ? (
            <div className="flex items-center gap-2 text-zinc-500">
              <span className="pulse-dot bg-zinc-400" />
              <span className="pulse-dot bg-zinc-400" style={{ animationDelay: '0.3s' }} />
              <span className="pulse-dot bg-zinc-400" style={{ animationDelay: '0.6s' }} />
              <span className="text-xs ml-1 font-medium">Thinking...</span>
            </div>
          ) : message.isReport ? (
            <ReportCard content={message.content} />
          ) : (
            <pre className="whitespace-pre-wrap font-sans custom-scrollbar overflow-x-auto">{message.content}</pre>
          )}
        </div>
      </div>
    </div>
  );
}

interface ChatPanelProps {
  messages: ChatMessage[];
  onOpenProject?: () => void;
  workspace?: string | null;
}

export default function ChatPanel({ messages, onOpenProject, workspace }: ChatPanelProps) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  return (
    <div className="flex flex-col flex-1 min-h-0 overflow-hidden text-zinc-800 dark:text-zinc-200">
      {/* Messages */}
      {/* Messages */}
      <div className={`flex-1 w-full px-4 sm:px-8 custom-scrollbar bg-transparent relative ${messages.length === 0 ? 'overflow-hidden flex items-center justify-center' : 'overflow-y-auto pt-6 pb-40 space-y-6'}`}>
        {messages.length === 0 ? (
          <div className="flex flex-col items-center justify-center w-full text-center relative z-10 -mt-16">
            {/* True 3D Particle / Smoke System */}
            <div className="absolute inset-0 -z-10 pointer-events-none flex justify-center items-center" style={{ perspective: '1000px' }}>
              <div className="relative w-full h-full" style={{ transformStyle: 'preserve-3d' }}>
                {/* Center backlight to highlight the chat icon */}
                <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[300px] h-[300px] bg-white/40 dark:bg-zinc-800/80 rounded-full filter blur-[60px]" style={{ transform: 'translate3d(-50%, -50%, -100px)' }}></div>

                {/* Dynamic 3D Particles generating a volumetric smoke field */}
                {Array.from({ length: 15 }).map((_, i) => {
                  // Generate random properties for true 3D depth and movement
                  const tx = (Math.random() - 0.5) * 1200; // Spread x
                  const ty = (Math.random() - 0.5) * 1000; // Spread y
                  const tz = (Math.random() - 0.5) * 800 + 100; // Spread z (depth)
                  const s = Math.random() * 1.5 + 0.5; // Scale variation
                  const duration = Math.random() * 10 + 15; // 15-25s duration
                  const delay = Math.random() * -25; // Random starting point in animation
                  const size = Math.random() * 200 + 100; // 100-300px size

                  // Color palette mixing (Violet, Fuchsia, Indigo/Sky)
                  const isViolet = i % 3 === 0;
                  const isFuchsia = i % 3 === 1;
                  const colorClass = isViolet ? 'bg-violet-400/20' : isFuchsia ? 'bg-fuchsia-400/20' : 'bg-sky-400/20';

                  return (
                    <div
                      key={i}
                      className={`absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 rounded-full filter blur-[60px] animate-particle mix-blend-screen dark:mix-blend-normal ${colorClass}`}
                      style={{
                        width: `${size}px`,
                        height: `${size}px`,
                        // @ts-ignore - Custom properties for the keyframe animation
                        '--tx': `${tx}px`,
                        '--ty': `${ty}px`,
                        '--tz': `${tz}px`,
                        '--s': s,
                        '--d': `${duration}s`,
                        '--del': `${delay}s`,
                      }}
                    />
                  );
                })}
              </div>
            </div>

            <div className="w-20 h-20 mb-8 rounded-[2rem] bg-white/40 dark:bg-zinc-800/40 backdrop-blur-xl border border-white/60 dark:border-zinc-700/50 shadow-[0_8px_32px_0_rgba(31,38,135,0.07)] dark:shadow-[0_8px_32px_0_rgba(0,0,0,0.2)] flex items-center justify-center transition-transform hover:scale-105 duration-300 animate-text-reveal">
              <AnimatedMessageIcon className="w-10 h-10 text-violet-500 drop-shadow-sm opacity-90" />
            </div>

            <p className="text-zinc-800 dark:text-zinc-100 text-3xl font-bold mb-3 tracking-tight animate-text-reveal delay-100">How can I help you today?</p>
            <p className="text-zinc-500 dark:text-zinc-400 text-[15px] max-w-md leading-relaxed mb-10 animate-text-reveal delay-200">
              Ask anything. I can plan architecture, write code, run terminal commands, and debug errors.
            </p>

            {/* Action cards */}
            <div className="flex gap-4 animate-text-reveal delay-200" style={{ animationDelay: '300ms' }}>
              <button
                onClick={onOpenProject}
                className="group flex flex-col items-start gap-3 px-5 py-4 w-48
                           rounded-3xl border border-white/50 dark:border-white/10
                           bg-white/40 dark:bg-zinc-800/40 backdrop-blur-xl
                           hover:bg-white/60 dark:hover:bg-zinc-700/40
                           hover:border-violet-300/50 dark:hover:border-violet-500/30
                           transition-all duration-300 shadow-[0_8px_32px_0_rgba(31,38,135,0.05)] 
                           hover:shadow-[0_8px_32px_0_rgba(31,38,135,0.1)] dark:shadow-[0_8px_32px_0_rgba(0,0,0,0.2)] 
                           hover:-translate-y-1 text-left relative overflow-hidden"
              >
                <div className="absolute inset-0 bg-gradient-to-br from-white/40 to-white/0 dark:from-white/5 pointer-events-none rounded-3xl"></div>
                <div className="w-10 h-10 rounded-xl bg-white/50 dark:bg-zinc-700/50 flex items-center justify-center border border-white/50 dark:border-zinc-600/50 shadow-sm mb-1 group-hover:scale-110 transition-transform duration-300">
                  <AnimatedFolderIcon className="w-6 h-6 text-zinc-600 dark:text-zinc-300" />
                </div>
                <div className="relative z-10 mt-1">
                  <p className="text-[15px] font-bold text-zinc-800 dark:text-zinc-100 group-hover:text-violet-700 dark:group-hover:text-violet-300 transition-colors">
                    Open Project
                  </p>
                  <p className="text-[12px] text-zinc-500 dark:text-zinc-400 mt-1 leading-snug font-medium">
                    {workspace ? `· ${workspace.split('/').pop()}` : 'Select a folder'}
                  </p>
                </div>
              </button>

              <div className="group flex flex-col items-start gap-3 px-5 py-4 w-48
                              rounded-3xl border border-white/50 dark:border-white/10
                              bg-white/30 dark:bg-zinc-800/30 backdrop-blur-xl
                              shadow-[0_8px_32px_0_rgba(31,38,135,0.03)] dark:shadow-[0_8px_32px_0_rgba(0,0,0,0.1)] 
                              text-left cursor-not-allowed relative overflow-hidden">
                <div className="absolute inset-0 bg-gradient-to-br from-white/40 to-white/0 dark:from-white/5 pointer-events-none rounded-3xl"></div>
                <div className="w-10 h-10 rounded-xl bg-white/50 dark:bg-zinc-700/50 flex items-center justify-center border border-white/50 dark:border-zinc-600/50 shadow-sm mb-1 opacity-70">
                  <AnimatedSparklesIcon className="w-6 h-6 text-amber-500 dark:text-amber-400" />
                </div>
                <div className="opacity-70 relative z-10 mt-1">
                  <p className="text-[15px] font-bold text-zinc-800 dark:text-zinc-100">New Project</p>
                  <p className="text-[12px] text-zinc-500 dark:text-zinc-400 mt-1 leading-snug font-medium">
                    Just describe it below
                  </p>
                </div>
              </div>
            </div>
          </div>
        ) : (
          <div className="max-w-4xl mx-auto w-full space-y-8">
            {messages.map((msg) => (
              <MessageBubble key={msg.id} message={msg} />
            ))}
            <div ref={bottomRef} className="h-px" />
          </div>
        )}
      </div>
    </div>
  );
}
