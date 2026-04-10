import { useEffect, useRef, useState, useCallback, useMemo } from 'react';
import { AnimatedMessageIcon, AnimatedFolderIcon, AnimatedSparklesIcon } from './icons/AnimatedIcons';
import { ChatMessage, AgentRole, ToolLog } from '../types';
import ReactMarkdown from 'react-markdown';
import rehypeSanitize from 'rehype-sanitize';
import remarkGfm from 'remark-gfm';
import { InlineToolCallGroup } from './InlineToolCall';
import InlineTodoList, { TodoItem } from './InlineTodoList';
import { ClaudeRoleIcon, CodexRoleIcon, DirectorRoleIcon } from './icons/RoleIcons';

// ── Role config ──────────────────────────────────────────────────────────────

const ROLE_CONFIG: Record<AgentRole, {
  label: string; fallbackIcon: string; color: string;
  gradient: string; glow: string; textGlow: string;
  roleIcon: ((size: number) => React.ReactNode) | null;
}> = {
  claude: {
    label: 'Claude',
    fallbackIcon: 'C',
    color: 'text-[#cc785c]',
    gradient: 'bg-gradient-to-br from-orange-400 to-amber-600',
    glow: 'shadow-[0_0_8px_rgba(204,120,92,0.4)]',
    textGlow: 'text-glow-claude',
    roleIcon: (s) => <ClaudeRoleIcon size={s} />,
  },
  codex: {
    label: 'Codex',
    fallbackIcon: 'X',
    color: 'text-[#10a37f]',
    gradient: 'bg-gradient-to-br from-emerald-400 to-teal-600',
    glow: 'shadow-[0_0_8px_rgba(16,163,127,0.4)]',
    textGlow: 'text-glow-codex',
    roleIcon: (s) => <CodexRoleIcon size={s} />,
  },
  director: {
    label: 'Director',
    fallbackIcon: 'D',
    color: 'text-themed-accent-text',
    gradient: 'bg-gradient-to-br from-violet-400 to-purple-600',
    glow: 'shadow-[0_0_8px_rgba(139,92,246,0.4)]',
    textGlow: 'text-glow-director',
    roleIcon: (s) => <DirectorRoleIcon size={s} />,
  },
  user: {
    label: 'You',
    fallbackIcon: 'U',
    color: 'text-content-primary',
    gradient: 'bg-gradient-to-br from-zinc-400 to-zinc-600',
    glow: '',
    textGlow: '',
    roleIcon: null,
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
  const timerRef = useRef<ReturnType<typeof setTimeout>>();

  // Clean up the timeout on unmount to avoid a React state-update warning.
  useEffect(() => () => clearTimeout(timerRef.current), []);

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => setCopied(false), 1500);
    }).catch(() => {
      // Clipboard access can be denied when window is not focused
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

// ── Markdown components for better rendering ─────────────────────────────────

const markdownComponents = {
  pre: ({ children }: { children: React.ReactNode }) => {
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
  table: ({ children }: { children: React.ReactNode }) => (
    <div className="my-2.5 overflow-x-auto rounded-lg border border-edge-primary/50">
      <table className="w-full text-[12px]">{children}</table>
    </div>
  ),
  thead: ({ children }: { children: React.ReactNode }) => (
    <thead className="bg-surface-tertiary/50 text-content-secondary">{children}</thead>
  ),
  th: ({ children }: { children: React.ReactNode }) => (
    <th className="px-3 py-1.5 text-left text-[11px] font-semibold border-b border-edge-primary/40">{children}</th>
  ),
  td: ({ children }: { children: React.ReactNode }) => (
    <td className="px-3 py-1.5 border-b border-edge-primary/20 text-content-primary">{children}</td>
  ),
  hr: () => (
    <hr className="my-3 border-edge-primary/40" />
  ),
};

// ── Message component ────────────────────────────────────────────────────────

function normalizeBubbleContent(content: string) {
  return content.replace(/^(?:\r?\n)+/, '').replace(/(?:\r?\n)+$/, '');
}

interface MessageProps {
  message: ChatMessage;
  isLast: boolean;
  /** Tool calls that happened between this message and the next. */
  toolCalls?: ToolLog[];
  /** Todo list snapshot to show after this message. */
  todos?: TodoItem[];
}

function Message({ message, isLast, toolCalls, todos }: MessageProps) {
  const config = ROLE_CONFIG[message.role];
  const isUser = message.role === 'user';
  const displayContent = normalizeBubbleContent(message.content);

  return (
    <div className={`animate-slide-up ${!isLast ? 'border-b border-edge-secondary/60' : ''}`}>
      <div className={`px-5 py-4 ${isUser ? 'bg-surface-secondary/20' : ''}`}>
        {/* Header: avatar + name + time */}
        <div className="flex items-center gap-2.5 mb-2">
          <div className={`flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-lg ${config.glow} ${config.roleIcon ? '' : `${config.gradient} text-[10px] font-bold text-white`}`}>
            {config.roleIcon ? config.roleIcon(18) : config.fallbackIcon}
          </div>
          <span className={`text-[12px] font-semibold ${config.textGlow || config.color}`}>{config.label}</span>
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
            <div className="chat-prose text-[13px] leading-[1.75] text-content-primary break-words overflow-hidden">
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                rehypePlugins={[rehypeSanitize]}
                components={markdownComponents}
              >
                {displayContent}
              </ReactMarkdown>
            </div>
          )}
        </div>

        {/* Inline tool calls after this message */}
        {toolCalls && toolCalls.length > 0 && (
          <InlineToolCallGroup logs={toolCalls} />
        )}

        {/* Inline todo list */}
        {todos && todos.length > 0 && (
          <InlineTodoList items={todos} />
        )}
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

// ── Subtask thread grouping ──────────────────────────────────────────────────

type MessageGroup =
  | { kind: 'flat'; message: ChatMessage }
  | { kind: 'thread'; subtaskId: string; label: string; messages: ChatMessage[] };

function groupBySubtask(messages: ChatMessage[]): MessageGroup[] {
  const groups: MessageGroup[] = [];
  const threadMap = new Map<string, Extract<MessageGroup, { kind: 'thread' }>>();

  for (const msg of messages) {
    if (msg.subtaskId) {
      const existing = threadMap.get(msg.subtaskId);
      if (existing) {
        existing.messages.push(msg);
      } else {
        const thread: Extract<MessageGroup, { kind: 'thread' }> = {
          kind: 'thread', subtaskId: msg.subtaskId,
          label: msg.subtaskLabel ?? msg.subtaskId, messages: [msg],
        };
        threadMap.set(msg.subtaskId, thread);
        groups.push(thread);
      }
    } else {
      groups.push({ kind: 'flat', message: msg });
    }
  }

  return groups;
}

/** Collapsible card that wraps all messages belonging to a single subtask. */
function SubtaskThread({ group, isLast, defaultExpanded }: { group: Extract<MessageGroup, { kind: 'thread' }>; isLast: boolean; defaultExpanded: boolean }) {
  const [expanded, setExpanded] = useState(defaultExpanded);
  const agentSet = [...new Set(group.messages.map(m => m.role))];
  const previewMsg = group.messages[group.messages.length - 1];

  return (
    <div className={`animate-slide-up px-4 py-2 ${!isLast ? 'border-b border-edge-secondary/60' : ''}`}>
      {/* Thread card with shimmer border */}
      <div className="shimmer-border rounded-xl bg-surface-secondary/30 backdrop-blur-sm">
        <button
          onClick={() => setExpanded(v => !v)}
          className="flex w-full items-center gap-2.5 px-4 py-3 text-left transition-colors hover:bg-surface-secondary/20 rounded-xl"
        >
          <svg
            className={`h-3.5 w-3.5 flex-shrink-0 text-content-tertiary transition-transform duration-200 ${expanded ? 'rotate-90' : ''}`}
            fill="none" stroke="currentColor" strokeWidth={2.5} viewBox="0 0 24 24"
          >
            <path strokeLinecap="round" strokeLinejoin="round" d="M9 5l7 7-7 7" />
          </svg>

          <span className="inline-flex items-center gap-1.5 rounded-lg bg-gradient-to-r from-violet-500/15 to-purple-500/10 px-2.5 py-1 text-[10.5px] font-semibold text-shimmer">
            <svg className="h-2.5 w-2.5 text-themed-accent-text" fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" d="M13.5 4.5L21 12m0 0l-7.5 7.5M21 12H3" /></svg>
            {group.label}
          </span>

          <div className="flex -space-x-1.5">
            {agentSet.map(role => {
              const cfg = ROLE_CONFIG[role];
              return (
                <div key={role} className={`flex h-[1.2rem] w-[1.2rem] items-center justify-center rounded-md ring-1 ring-surface-primary/50 ${cfg.roleIcon ? '' : `${cfg.gradient} text-[8px] font-bold text-white`} ${cfg.glow}`}>
                  {cfg.roleIcon ? cfg.roleIcon(14) : cfg.fallbackIcon}
                </div>
              );
            })}
          </div>

          <span className="text-[10px] text-content-tertiary">{group.messages.length} msgs</span>

          {!expanded && previewMsg && (
            <span className="ml-auto max-w-[40%] truncate text-[11px] text-content-tertiary">
              {previewMsg.content.slice(0, 80)}
            </span>
          )}
        </button>

        {expanded && (
          <div className="border-t border-edge-primary/30 mx-2">
            {group.messages.map((msg, idx) => (
              <Message key={msg.id} message={msg} isLast={idx === group.messages.length - 1} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

// ── Interleave tool calls with messages ─────────────────────────────────────

/**
 * Assign tool calls to the message they belong after.
 * Tool calls between message[i] and message[i+1] are grouped with message[i].
 */
function assignToolCallsToMessages(
  messages: ChatMessage[],
  toolLogs: ToolLog[],
): Map<string, ToolLog[]> {
  const map = new Map<string, ToolLog[]>();
  if (toolLogs.length === 0) return map;

  // Sort messages by timestamp
  const sorted = [...messages].filter(m => !m.subtaskId).sort((a, b) => a.timestamp - b.timestamp);
  if (sorted.length === 0) return map;

  for (const log of toolLogs) {
    // Find the last message whose timestamp <= this log's timestamp
    let assignTo: ChatMessage | null = null;
    for (let i = sorted.length - 1; i >= 0; i--) {
      if (sorted[i].timestamp <= log.timestamp && sorted[i].role !== 'user') {
        assignTo = sorted[i];
        break;
      }
    }
    if (assignTo) {
      const existing = map.get(assignTo.id) ?? [];
      existing.push(log);
      map.set(assignTo.id, existing);
    }
  }

  return map;
}

// ── ChatPanel ────────────────────────────────────────────────────────────────

interface ChatPanelProps {
  messages: ChatMessage[];
  toolLogs?: ToolLog[];
  todos?: TodoItem[];
  onOpenProject?: () => void;
  workspace?: string | null;
}

export default function ChatPanel({ messages, toolLogs = [], todos = [], onOpenProject, workspace }: ChatPanelProps) {
  const bottomRef = useRef<HTMLDivElement>(null);
  const [heroOrbs] = useState(() => [
    { id: 0, color: 'rgba(139,92,246,0.18)', size: 340, x: -120, y: -80,  dur: '20s', del: '0s' },
    { id: 1, color: 'rgba(236,72,153,0.12)', size: 280, x:  140, y: -60,  dur: '24s', del: '-6s' },
    { id: 2, color: 'rgba(56,189,248,0.14)', size: 300, x:   20, y:  100, dur: '22s', del: '-3s' },
    { id: 3, color: 'rgba(167,139,250,0.10)', size: 220, x: -180, y:  60, dur: '26s', del: '-9s' },
    { id: 4, color: 'rgba(244,114,182,0.08)', size: 260, x:  200, y:  80, dur: '18s', del: '-12s' },
  ]);

  const messageGroups = useMemo(() => groupBySubtask(messages), [messages]);

  // Assign tool logs to their nearest preceding message
  const toolCallMap = useMemo(
    () => assignToolCallsToMessages(messages, toolLogs),
    [messages, toolLogs],
  );

  // Show todos on the last non-user message
  const lastAgentMsgId = useMemo(() => {
    for (let i = messages.length - 1; i >= 0; i--) {
      if (messages[i].role !== 'user' && !messages[i].subtaskId) return messages[i].id;
    }
    return null;
  }, [messages]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, toolLogs]);

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

            <div className="mb-8 animate-text-reveal">
              <AnimatedMessageIcon className="w-14 h-14 drop-shadow-sm" />
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
                  <AnimatedFolderIcon className="h-7 w-7" />
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
                  <AnimatedSparklesIcon className="h-7 w-7" />
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
            {messageGroups.map((group, idx) => {
              const isLast = idx === messageGroups.length - 1;
              if (group.kind === 'flat') {
                const msgToolCalls = toolCallMap.get(group.message.id);
                const showTodos = group.message.id === lastAgentMsgId && todos.length > 0 ? todos : undefined;
                return (
                  <Message
                    key={group.message.id}
                    message={group.message}
                    isLast={isLast}
                    toolCalls={msgToolCalls}
                    todos={showTodos}
                  />
                );
              }
              return (
                <SubtaskThread
                  key={`thread-${group.subtaskId}`}
                  group={group}
                  isLast={isLast}
                  defaultExpanded={isLast}
                />
              );
            })}
            <div ref={bottomRef} className="h-px" />
          </div>
        )}
      </div>
    </div>
  );
}
