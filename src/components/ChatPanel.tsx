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
    color: 'text-[#cc785c]',
    borderColor: 'border-[#cc785c]/25',
  },
  codex: {
    label: 'Codex',
    icon: 'X',
    color: 'text-[#10a37f]',
    borderColor: 'border-[#10a37f]/25',
  },
  director: {
    label: 'Director',
    icon: 'D',
    color: 'text-themed-accent-text',
    borderColor: 'border-themed-accent/25',
  },
  user: {
    label: 'You',
    icon: 'U',
    color: 'text-content-primary',
    borderColor: 'border-edge-primary',
  },
};

// ── Report card (collapsible plan documents) ─────────────────────────────────

function ReportCard({ content }: { content: string }) {
  const [expanded, setExpanded] = useState(false);
  const title = content.split('\n')[0].replace(/^#+\s*/, '');
  const preview = content.split('\n').slice(1, 6).join('\n').trim();

  return (
    <div className="overflow-hidden rounded-xl border border-themed-accent/20 shadow-sm" style={{ backgroundColor: 'rgb(var(--bg-elevated) / 0.8)' }}>
      <div className="flex items-center justify-between border-b border-themed-accent/15 px-3.5 py-2" style={{ backgroundColor: 'rgb(var(--accent-soft))' }}>
        <div className="flex items-center gap-2">
          <svg className="h-3 w-3 text-themed-accent-text" fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m2.25 0H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" /></svg>
          <span className="text-[11px] font-semibold text-themed-accent-text">{title}</span>
        </div>
        <button
          onClick={() => setExpanded(v => !v)}
          className="text-[11px] font-medium text-themed-accent-text/80 hover:text-themed-accent-text"
        >
          {expanded ? 'Collapse' : 'View Report'}
        </button>
      </div>
      {expanded ? (
        <pre className="custom-scrollbar m-0 max-h-[60vh] overflow-y-auto whitespace-pre-wrap p-4 font-mono text-[11px] leading-6 text-content-primary" style={{ backgroundColor: 'rgb(var(--bg-elevated))' }}>
          {content}
        </pre>
      ) : (
        <div className="px-3.5 py-2.5" style={{ backgroundColor: 'rgb(var(--bg-elevated))' }}>
          <pre className="m-0 line-clamp-3 whitespace-pre-wrap font-mono text-[11px] text-content-tertiary">
            {preview}
          </pre>
          <button
            onClick={() => setExpanded(true)}
            className="mt-1.5 text-[11px] font-medium text-themed-accent-text/80 hover:text-themed-accent-text"
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
  const [heroOrbs] = useState(() => [
    { id: 0, color: 'rgba(139,92,246,0.18)', size: 340, x: -120, y: -80,  dur: '20s', del: '0s' },
    { id: 1, color: 'rgba(236,72,153,0.12)', size: 280, x:  140, y: -60,  dur: '24s', del: '-6s' },
    { id: 2, color: 'rgba(56,189,248,0.14)', size: 300, x:   20, y:  100, dur: '22s', del: '-3s' },
    { id: 3, color: 'rgba(167,139,250,0.10)', size: 220, x: -180, y:  60, dur: '26s', del: '-9s' },
    { id: 4, color: 'rgba(244,114,182,0.08)', size: 260, x:  200, y:  80, dur: '18s', del: '-12s' },
  ]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  return (
    <div className="flex flex-col flex-1 min-h-0 overflow-hidden text-content-primary">
      <div className={`custom-scrollbar relative flex-1 w-full bg-transparent ${messages.length === 0 ? 'overflow-hidden flex items-center justify-center' : 'overflow-y-auto pb-36'}`}>
        {messages.length === 0 ? (
          <div className="flex flex-col items-center justify-center w-full text-center relative z-10 -mt-16">
            {/* Aurora gradient mesh */}
            <div className="absolute inset-0 -z-10 pointer-events-none overflow-hidden">
              {heroOrbs.map((orb) => (
                <div
                  key={orb.id}
                  className="absolute top-1/2 left-1/2 rounded-full animate-aurora-orb"
                  style={{
                    width: orb.size,
                    height: orb.size,
                    background: `radial-gradient(circle, ${orb.color}, transparent 70%)`,
                    filter: 'blur(60px)',
                    // @ts-ignore
                    '--orb-x': `${orb.x}px`,
                    '--orb-y': `${orb.y}px`,
                    animationDuration: orb.dur,
                    animationDelay: orb.del,
                  }}
                />
              ))}
            </div>

            <div className="mb-8 flex h-20 w-20 items-center justify-center rounded-[2rem] border border-edge-primary/40 backdrop-blur-xl transition-transform duration-300 hover:scale-[1.03] animate-text-reveal" style={{ backgroundColor: 'rgb(var(--bg-elevated) / 0.5)', boxShadow: '0 8px 32px rgb(var(--bg-primary) / 0.12)' }}>
              <AnimatedMessageIcon className="w-10 h-10 drop-shadow-sm opacity-90" />
            </div>

            <p className="text-content-primary text-3xl font-bold mb-3 tracking-tight animate-text-reveal delay-100">What are we building?</p>
            <p className="text-content-secondary text-[15px] max-w-md leading-relaxed mb-10 animate-text-reveal delay-200">
              Describe your idea and I'll orchestrate planning, coding, testing, and review — end to end.
            </p>

            {/* Action cards */}
            <div className="grid w-full max-w-[36.5rem] grid-cols-1 gap-3 sm:grid-cols-2 animate-text-reveal delay-200" style={{ animationDelay: '300ms' }}>
              <button
                onClick={onOpenProject}
                className="group relative min-h-[12.4rem] overflow-hidden rounded-[1.65rem] border border-edge-primary/50 p-4 text-left backdrop-blur-2xl transition-all duration-300 hover:-translate-y-1 hover:border-sky-400/50"
                style={{ backgroundColor: 'rgb(var(--bg-elevated) / 0.55)', boxShadow: '0 16px 40px rgb(var(--bg-primary) / 0.1)' }}
              >
                <div className="absolute inset-0 rounded-[1.65rem] bg-[radial-gradient(circle_at_top_left,rgba(125,211,252,0.18),transparent_42%),radial-gradient(circle_at_bottom_right,rgba(59,130,246,0.12),transparent_36%)] opacity-90 pointer-events-none" />
                <div className="relative z-10 flex h-full flex-col">
                  <div className="flex h-9 w-9 items-center justify-center rounded-[1rem] border border-edge-primary/60 bg-surface-elevated/80 shadow-sm transition-transform duration-300 group-hover:scale-105">
                    <AnimatedFolderIcon className="h-5 w-5" />
                  </div>
                  <div className="mt-5">
                    <p className="text-[1.28rem] font-semibold tracking-[-0.03em] text-content-primary">
                      Open Project
                    </p>
                    <p className="mt-1.5 text-[12.5px] leading-5.5 text-content-secondary">
                      {workspace ? `Continue working on ${workspace.split('/').pop()}` : 'Load an existing codebase to plan features, fix bugs, or refactor.'}
                    </p>
                  </div>
                </div>
              </button>

              <div
                className="group relative min-h-[12.4rem] overflow-hidden rounded-[1.65rem] border border-edge-primary/40 p-4 text-left backdrop-blur-2xl"
                style={{ backgroundColor: 'rgb(var(--bg-elevated) / 0.42)', boxShadow: '0 16px 40px rgb(var(--bg-primary) / 0.08)' }}
              >
                <div className="absolute inset-0 rounded-[1.65rem] bg-[radial-gradient(circle_at_top_left,rgba(251,191,36,0.16),transparent_42%),radial-gradient(circle_at_bottom_right,rgba(249,115,22,0.1),transparent_36%)] opacity-90 pointer-events-none" />
                <div className="relative z-10 flex h-full flex-col opacity-90">
                  <div className="flex h-9 w-9 items-center justify-center rounded-[1rem] border border-edge-primary/60 bg-surface-elevated/80 shadow-sm">
                    <AnimatedSparklesIcon className="h-5 w-5" />
                  </div>
                  <div className="mt-5">
                    <p className="text-[1.28rem] font-semibold tracking-[-0.03em] text-content-primary">
                      Start Fresh
                    </p>
                    <p className="mt-1.5 text-[12.5px] leading-5.5 text-content-secondary">
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
