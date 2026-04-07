import { useEffect, useRef, useState, useCallback } from 'react';
import { AnimatedMessageIcon, AnimatedFolderIcon, AnimatedSparklesIcon } from './icons/AnimatedIcons';
import { ChatMessage, AgentRole } from '../types';
import ReactMarkdown from 'react-markdown';
import rehypeSanitize from 'rehype-sanitize';
import remarkGfm from 'remark-gfm';

// ── Role config ──────────────────────────────────────────────────────────────

const ROLE_CONFIG: Record<AgentRole, { label: string; icon: string; color: string; borderColor: string }> = {
  claude: {
    label: 'Claude',
    icon: 'C',
    color: 'text-orange-600 dark:text-orange-400',
    borderColor: 'border-orange-200 dark:border-orange-500/30',
  },
  codex: {
    label: 'Codex',
    icon: 'X',
    color: 'text-emerald-600 dark:text-emerald-400',
    borderColor: 'border-emerald-200 dark:border-emerald-500/30',
  },
  director: {
    label: 'Director',
    icon: 'D',
    color: 'text-violet-600 dark:text-violet-400',
    borderColor: 'border-violet-200 dark:border-violet-500/30',
  },
  user: {
    label: 'You',
    icon: 'U',
    color: 'text-zinc-600 dark:text-zinc-300',
    borderColor: 'border-zinc-200 dark:border-zinc-700',
  },
};

// ── Report card (collapsible plan documents) ─────────────────────────────────

function ReportCard({ content }: { content: string }) {
  const [expanded, setExpanded] = useState(false);
  const title = content.split('\n')[0].replace(/^#+\s*/, '');
  const preview = content.split('\n').slice(1, 6).join('\n').trim();

  return (
    <div className="overflow-hidden rounded-xl border border-violet-200/70 bg-white/80 shadow-sm dark:border-violet-500/20 dark:bg-zinc-900/80">
      <div className="flex items-center justify-between border-b border-violet-200/60 bg-violet-50/60 px-3.5 py-2 dark:border-violet-500/15 dark:bg-violet-500/8">
        <div className="flex items-center gap-2">
          <svg className="h-3 w-3 text-violet-500" fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m2.25 0H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" /></svg>
          <span className="text-[11px] font-semibold text-violet-700 dark:text-violet-300">{title}</span>
        </div>
        <button
          onClick={() => setExpanded(v => !v)}
          className="text-[11px] font-medium text-violet-500 hover:text-violet-700 dark:text-violet-400 dark:hover:text-violet-300"
        >
          {expanded ? 'Collapse' : 'View Report'}
        </button>
      </div>
      {expanded ? (
        <pre className="custom-scrollbar m-0 max-h-[60vh] overflow-y-auto whitespace-pre-wrap bg-white p-4 font-mono text-[11px] leading-6 text-zinc-700 dark:bg-zinc-900 dark:text-zinc-300">
          {content}
        </pre>
      ) : (
        <div className="px-3.5 py-2.5 bg-white dark:bg-zinc-900">
          <pre className="m-0 line-clamp-3 whitespace-pre-wrap font-mono text-[11px] text-zinc-400 dark:text-zinc-500">
            {preview}
          </pre>
          <button
            onClick={() => setExpanded(true)}
            className="mt-1.5 text-[11px] font-medium text-violet-500 hover:text-violet-700 dark:hover:text-violet-300"
          >
            Click to view full plan →
          </button>
        </div>
      )}
    </div>
  );
}

// ── Code block with copy button ──────────────────────────────────────────────

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  }, [text]);

  return (
    <button
      onClick={handleCopy}
      className="absolute right-2 top-2 rounded-md border border-edge-primary/60 bg-surface-elevated/80 px-1.5 py-0.5 text-[10px] font-medium text-content-secondary opacity-0 transition-all hover:bg-surface-tertiary group-hover:opacity-100"
    >
      {copied ? '✓ Copied' : 'Copy'}
    </button>
  );
}

// ── Message component (Cursor-style) ─────────────────────────────────────────

function normalizeBubbleContent(content: string) {
  return content.replace(/^(?:\r?\n)+/, '').replace(/(?:\r?\n)+$/, '');
}

interface MessageProps {
  message: ChatMessage;
  isLast: boolean;
}

function Message({ message, isLast }: MessageProps) {
  const config = ROLE_CONFIG[message.role];
  const isUser = message.role === 'user';
  const displayContent = normalizeBubbleContent(message.content);

  return (
    <div className={`animate-slide-up ${!isLast ? 'border-b border-edge-secondary' : ''}`}>
      <div className={`px-5 py-4 ${isUser ? 'bg-surface-secondary/20' : ''}`}>
        {/* Header: avatar + name + time */}
        <div className="flex items-center gap-2.5 mb-2.5">
          <div className={`flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-lg border text-[10px] font-bold ${config.borderColor} bg-surface-elevated/70`}>
            <span className={config.color}>{config.icon}</span>
          </div>
          <span className={`text-[12px] font-semibold ${config.color}`}>{config.label}</span>
          <span className="text-[10px] text-content-tertiary tabular-nums">
            {new Date(message.timestamp).toLocaleTimeString('en-US', {
              hour: 'numeric',
              minute: '2-digit',
              hour12: true,
            })}
          </span>
        </div>

        {/* Content */}
        <div className="pl-[34px]">
          {message.thinking ? (
            <div className="flex items-center gap-1.5 text-content-tertiary">
              <div className="flex gap-1">
                <span className="h-1.5 w-1.5 rounded-full bg-content-tertiary animate-pulse" />
                <span className="h-1.5 w-1.5 rounded-full bg-content-tertiary animate-pulse" style={{ animationDelay: '0.3s' }} />
                <span className="h-1.5 w-1.5 rounded-full bg-content-tertiary animate-pulse" style={{ animationDelay: '0.6s' }} />
              </div>
              <span className="text-[11px] font-medium">Thinking...</span>
            </div>
          ) : message.isReport ? (
            <ReportCard content={message.content} />
          ) : (
            <div className="chat-prose text-[13.5px] leading-[1.7] text-content-primary">
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                rehypePlugins={[rehypeSanitize]}
                components={{
                  pre: ({ children }) => {
                    // Extract text from code children for copy button
                    const codeText = extractText(children);
                    return (
                      <div className="group relative my-2.5">
                        <pre className="overflow-x-auto rounded-xl border border-edge-primary/60 px-3.5 py-3 font-mono text-[12px] leading-relaxed text-content-primary" style={{ backgroundColor: 'rgb(var(--bg-tertiary) / 0.7)' }}>
                          {children}
                        </pre>
                        <CopyButton text={codeText} />
                      </div>
                    );
                  },
                }}
              >
                {displayContent}
              </ReactMarkdown>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

/** Recursively extract text content from React children (for copy button). */
function extractText(node: React.ReactNode): string {
  if (typeof node === 'string') return node;
  if (typeof node === 'number') return String(node);
  if (!node) return '';
  if (Array.isArray(node)) return node.map(extractText).join('');
  if (typeof node === 'object' && node !== null && 'props' in node) {
    const el = node as { props?: { children?: React.ReactNode } };
    return extractText(el.props?.children);
  }
  return '';
}

// ── ChatPanel ────────────────────────────────────────────────────────────────

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
      <div className={`custom-scrollbar relative flex-1 w-full bg-transparent ${messages.length === 0 ? 'overflow-hidden flex items-center justify-center' : 'overflow-y-auto pb-36'}`}>
        {messages.length === 0 ? (
          <div className="flex flex-col items-center justify-center w-full text-center relative z-10 -mt-16">
            {/* 3D Particle System */}
            <div className="absolute inset-0 -z-10 pointer-events-none flex justify-center items-center" style={{ perspective: '1000px' }}>
              <div className="relative w-full h-full" style={{ transformStyle: 'preserve-3d' }}>
                <div className="absolute top-1/2 left-1/2 h-[260px] w-[260px] -translate-x-1/2 -translate-y-1/2 rounded-full bg-white/35 filter blur-[54px] dark:bg-zinc-800/70" style={{ transform: 'translate3d(-50%, -50%, -100px)' }} />
                {heroParticles.map((particle) => (
                  <div
                    key={particle.id}
                    className={`absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 rounded-full filter blur-[58px] animate-particle mix-blend-screen dark:mix-blend-normal ${particle.colorClass}`}
                    style={{
                      width: particle.size,
                      height: particle.size,
                      // @ts-ignore
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

            <p className="text-content-primary text-3xl font-bold mb-3 tracking-tight animate-text-reveal delay-100">What are we building?</p>
            <p className="text-content-secondary text-[15px] max-w-md leading-relaxed mb-10 animate-text-reveal delay-200">
              Describe your idea and I'll orchestrate planning, coding, testing, and review — end to end.
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
                      {workspace ? `Continue working on ${workspace.split('/').pop()}` : 'Load an existing codebase to plan features, fix bugs, or refactor.'}
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
                      Start Fresh
                    </p>
                    <p className="mt-1.5 text-[12.5px] leading-5.5 text-zinc-600 dark:text-zinc-300">
                      Describe what you want to build. Director will create a plan and scaffold the project.
                    </p>
                  </div>
                </div>
              </div>
            </div>
          </div>
        ) : (
          <div className="mx-auto w-full max-w-[52rem]">
            {messages.map((msg, idx) => (
              <Message key={msg.id} message={msg} isLast={idx === messages.length - 1} />
            ))}
            <div ref={bottomRef} className="h-px" />
          </div>
        )}
      </div>
    </div>
  );
}
