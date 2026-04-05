import { useEffect, useRef, useState } from 'react';
import { AnimatedMessageIcon, AnimatedFolderIcon, AnimatedSparklesIcon } from './icons/AnimatedIcons';
import { ChatMessage, AgentRole } from '../types';

const ROLE_CONFIG: Record<AgentRole, { label: string; color: string; bg: string }> = {
  claude: {
    label: 'Claude',
    color: 'text-orange-600 dark:text-accent-claude',
    bg: 'bg-orange-100/60 dark:bg-accent-claude/20 border-orange-200/60 dark:border-accent-claude/30 backdrop-blur-md',
  },
  codex: {
    label: 'Codex',
    color: 'text-emerald-600 dark:text-accent-codex',
    bg: 'bg-emerald-100/60 dark:bg-accent-codex/20 border-emerald-200/60 dark:border-accent-codex/30 backdrop-blur-md',
  },
  director: {
    label: 'Director',
    color: 'text-violet-600 dark:text-violet-400',
    bg: 'bg-violet-100/60 dark:bg-violet-500/20 border-violet-200/60 dark:border-violet-500/30 backdrop-blur-md',
  },
  user: {
    label: 'You',
    color: 'text-zinc-700 dark:text-zinc-300',
    bg: 'bg-zinc-200/60 dark:bg-zinc-700/60 border-zinc-300/60 dark:border-zinc-600/60 backdrop-blur-md',
  },
};

function ReportCard({ content }: { content: string }) {
  const [expanded, setExpanded] = useState(false);
  const title = content.split('\n')[0].replace(/^#+\s*/, '');
  const preview = content.split('\n').slice(1, 6).join('\n').trim();

  return (
    <div className="w-full overflow-hidden rounded-2xl border border-violet-200/70 bg-white/80 shadow-[0_10px_28px_rgba(0,0,0,0.05)] dark:border-violet-500/30 dark:bg-zinc-900/80 dark:shadow-[0_10px_28px_rgba(0,0,0,0.28)]">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-violet-200/80 bg-violet-50/90 px-4 py-2.5 dark:border-violet-500/20 dark:bg-violet-500/10">
        <div className="flex items-center gap-2">
          <span>📄</span>
          <span className="text-[11px] font-semibold tracking-[0.01em] text-violet-700 dark:text-violet-300">{title}</span>
        </div>
        <button
          onClick={() => setExpanded(v => !v)}
          className="text-[11px] font-medium text-violet-600 hover:underline dark:text-violet-400"
        >
          {expanded ? 'Collapse' : 'View Report'}
        </button>
      </div>

      {/* Body */}
      {expanded ? (
        <pre className="custom-scrollbar m-0 max-h-[60vh] overflow-y-auto whitespace-pre-wrap bg-white p-4 font-mono text-[11px] leading-6 text-zinc-700 dark:bg-zinc-900 dark:text-zinc-300">
          {content}
        </pre>
      ) : (
        <div className="px-4 py-3 bg-white dark:bg-zinc-900">
          <pre className="m-0 line-clamp-3 whitespace-pre-wrap font-mono text-[11px] text-zinc-400 dark:text-zinc-500">
            {preview}
          </pre>
          <button
            onClick={() => setExpanded(true)}
            className="mt-2 text-[11px] font-medium text-violet-500 hover:text-violet-700 dark:hover:text-violet-300"
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

function normalizeBubbleContent(content: string) {
  return content.replace(/^(?:\r?\n)+/, '').replace(/(?:\r?\n)+$/, '');
}

function MessageBubble({ message }: MessageBubbleProps) {
  const config = ROLE_CONFIG[message.role];
  const isUser = message.role === 'user';
  const displayContent = normalizeBubbleContent(message.content);

  return (
    <div className={`flex gap-2.5 animate-slide-up ${isUser ? 'flex-row-reverse' : 'flex-row'}`}>
      {/* Avatar */}
      <div className={`flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-2xl border text-[11px] font-bold ${config.bg} shadow-sm`}>
        <span className={config.color}>
          {message.role === 'claude' ? 'C' :
            message.role === 'codex' ? 'X' :
              message.role === 'director' ? 'D' : 'U'}
        </span>
      </div>

      <div className={`flex w-full max-w-[46rem] flex-col space-y-1 ${isUser ? 'items-end' : 'items-start'}`}>
        <div className={`flex items-center gap-1.5 ${isUser ? 'flex-row-reverse' : 'flex-row'}`}>
          <span className={`text-[11px] font-semibold tracking-[0.01em] ${config.color}`}>{config.label}</span>
          <span className="text-[10px] text-zinc-400 dark:text-zinc-500">
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
              <span className="ml-1 text-[11px] font-medium">Thinking...</span>
            </div>
          ) : message.isReport ? (
            <ReportCard content={message.content} />
          ) : (
            <div className="custom-scrollbar m-0 overflow-x-auto whitespace-pre-wrap break-words font-sans text-[13px] leading-[1.55] sm:text-[14px]">
              {displayContent}
            </div>
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
  const [heroParticles] = useState(() =>
    Array.from({ length: 9 }, (_, i) => ({
      id: i,
      tx: (Math.sin(i * 1.7) * 420).toFixed(1),
      ty: (Math.cos(i * 1.3) * 300).toFixed(1),
      tz: (120 + (i % 5) * 55).toFixed(1),
      scale: (0.78 + (i % 4) * 0.18).toFixed(2),
      duration: `${15 + (i % 4) * 2.5}s`,
      delay: `${-i * 1.8}s`,
      size: `${150 + (i % 4) * 36}px`,
      colorClass: i % 3 === 0 ? 'bg-violet-400/18' : i % 3 === 1 ? 'bg-fuchsia-400/16' : 'bg-sky-400/18',
    })),
  );

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  return (
    <div className="flex flex-col flex-1 min-h-0 overflow-hidden text-zinc-800 dark:text-zinc-200">
      {/* Messages */}
      {/* Messages */}
      <div className={`custom-scrollbar relative flex-1 w-full bg-transparent px-4 sm:px-7 ${messages.length === 0 ? 'overflow-hidden flex items-center justify-center' : 'overflow-y-auto pt-5 pb-36 space-y-5'}`}>
        {messages.length === 0 ? (
          <div className="flex flex-col items-center justify-center w-full text-center relative z-10 -mt-16">
            {/* True 3D Particle / Smoke System */}
            <div className="absolute inset-0 -z-10 pointer-events-none flex justify-center items-center" style={{ perspective: '1000px' }}>
              <div className="relative w-full h-full" style={{ transformStyle: 'preserve-3d' }}>
                {/* Center backlight to highlight the chat icon */}
                <div className="absolute top-1/2 left-1/2 h-[260px] w-[260px] -translate-x-1/2 -translate-y-1/2 rounded-full bg-white/35 filter blur-[54px] dark:bg-zinc-800/70" style={{ transform: 'translate3d(-50%, -50%, -100px)' }} />

                {/* Dynamic 3D Particles generating a volumetric smoke field */}
                {heroParticles.map((particle) => (
                  <div
                    key={particle.id}
                    className={`absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 rounded-full filter blur-[58px] animate-particle mix-blend-screen dark:mix-blend-normal ${particle.colorClass}`}
                    style={{
                      width: particle.size,
                      height: particle.size,
                      // @ts-ignore - Custom properties for the keyframe animation
                      '--tx': `${particle.tx}px`,
                      '--ty': `${particle.ty}px`,
                      '--tz': `${particle.tz}px`,
                      '--s': particle.scale,
                      '--d': particle.duration,
                      '--del': particle.delay,
                    }}
                  />
                ))}
              </div>
            </div>

            <div className="mb-8 flex h-20 w-20 items-center justify-center rounded-[2rem] border border-white/60 bg-white/40 shadow-[0_8px_32px_0_rgba(31,38,135,0.07)] backdrop-blur-xl transition-transform duration-300 hover:scale-[1.03] dark:border-zinc-700/50 dark:bg-zinc-800/40 dark:shadow-[0_8px_32px_0_rgba(0,0,0,0.2)] animate-text-reveal">
              <AnimatedMessageIcon className="w-10 h-10 text-violet-500 drop-shadow-sm opacity-90" />
            </div>

            <p className="text-zinc-800 dark:text-zinc-100 text-3xl font-bold mb-3 tracking-tight animate-text-reveal delay-100">How can I help you today?</p>
            <p className="text-zinc-500 dark:text-zinc-400 text-[15px] max-w-md leading-relaxed mb-10 animate-text-reveal delay-200">
              Ask anything. I can plan architecture, write code, run terminal commands, and debug errors.
            </p>

            {/* Action cards */}
            <div className="grid w-full max-w-[36.5rem] grid-cols-1 gap-3 sm:grid-cols-2 animate-text-reveal delay-200" style={{ animationDelay: '300ms' }}>
              <button
                onClick={onOpenProject}
                className="group relative min-h-[12.4rem] overflow-hidden rounded-[1.65rem] border border-white/70 bg-white/55 p-4 text-left shadow-[0_16px_40px_rgba(15,23,42,0.07)] backdrop-blur-2xl transition-all duration-300 hover:-translate-y-1 hover:border-sky-300/70 hover:bg-white/72 hover:shadow-[0_20px_50px_rgba(59,130,246,0.13)] dark:border-white/10 dark:bg-zinc-900/38 dark:hover:border-sky-500/30 dark:hover:bg-zinc-900/52 dark:hover:shadow-[0_20px_50px_rgba(0,0,0,0.3)]"
              >
                <div className="absolute inset-0 rounded-[1.65rem] bg-[radial-gradient(circle_at_top_left,rgba(125,211,252,0.22),transparent_42%),radial-gradient(circle_at_bottom_right,rgba(59,130,246,0.16),transparent_36%)] opacity-90 pointer-events-none dark:bg-[radial-gradient(circle_at_top_left,rgba(56,189,248,0.18),transparent_42%),radial-gradient(circle_at_bottom_right,rgba(59,130,246,0.16),transparent_36%)]" />
                <div className="relative z-10 flex h-full flex-col">
                  <div className="flex h-9 w-9 items-center justify-center rounded-[1rem] border border-white/80 bg-white/85 shadow-[0_8px_20px_rgba(14,165,233,0.14)] transition-transform duration-300 group-hover:scale-105 dark:border-sky-500/30 dark:bg-sky-500/10 dark:shadow-[0_10px_24px_rgba(14,165,233,0.1)]">
                    <AnimatedFolderIcon className="h-5 w-5 text-sky-700 dark:text-sky-300" />
                  </div>
                  <div className="mt-5">
                    <p className="text-[1.28rem] font-semibold tracking-[-0.03em] text-zinc-900 dark:text-zinc-50">
                      Open Project
                    </p>
                    <p className="mt-1.5 text-[12.5px] leading-5.5 text-zinc-600 dark:text-zinc-300">
                      {workspace ? `Continue in ${workspace.split('/').pop()}` : 'Choose a local folder and load its workspace context.'}
                    </p>
                  </div>
                </div>
              </button>

              <div className="group relative min-h-[12.4rem] overflow-hidden rounded-[1.65rem] border border-white/60 bg-white/42 p-4 text-left shadow-[0_16px_40px_rgba(15,23,42,0.05)] backdrop-blur-2xl dark:border-white/10 dark:bg-zinc-900/30 dark:shadow-[0_16px_40px_rgba(0,0,0,0.18)]">
                <div className="absolute inset-0 rounded-[1.65rem] bg-[radial-gradient(circle_at_top_left,rgba(251,191,36,0.2),transparent_42%),radial-gradient(circle_at_bottom_right,rgba(249,115,22,0.14),transparent_36%)] opacity-90 pointer-events-none dark:bg-[radial-gradient(circle_at_top_left,rgba(245,158,11,0.16),transparent_42%),radial-gradient(circle_at_bottom_right,rgba(249,115,22,0.14),transparent_36%)]" />
                <div className="relative z-10 flex h-full flex-col opacity-90">
                  <div className="flex h-9 w-9 items-center justify-center rounded-[1rem] border border-white/80 bg-white/82 shadow-[0_8px_20px_rgba(245,158,11,0.12)] dark:border-amber-500/30 dark:bg-amber-500/10 dark:shadow-[0_10px_24px_rgba(245,158,11,0.08)]">
                    <AnimatedSparklesIcon className="h-5 w-5 text-amber-600 dark:text-amber-300" />
                  </div>
                  <div className="mt-5">
                    <p className="text-[1.28rem] font-semibold tracking-[-0.03em] text-zinc-900 dark:text-zinc-50">
                      New Project
                    </p>
                    <p className="mt-1.5 text-[12.5px] leading-5.5 text-zinc-600 dark:text-zinc-300">
                      Describe the product, target users, and constraints below to start from a blank brief.
                    </p>
                  </div>
                </div>
              </div>
            </div>
          </div>
        ) : (
          <div className="mx-auto w-full max-w-[54rem] space-y-6">
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
