import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { VscArrowLeft, VscChecklist, VscChevronDown, VscChevronRight, VscChevronUp, VscLayoutSidebarRightOff, VscRefresh } from 'react-icons/vsc';
import { BlackboardEvent } from '../types';
import ReactMarkdown, { Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeSanitize from 'rehype-sanitize';

interface BlackboardPanelProps {
  workspacePath: string | null;
  events: BlackboardEvent[];
  onClose: () => void;
  onBack?: () => void;
  fullscreen?: boolean;
  unreadAgentMessages?: number;
}

type BoardState = 'pending' | 'in_progress' | 'completed' | 'failed';
type SubtaskState = 'pending' | 'in_progress' | 'needs_fix' | 'done' | 'failed';
type ExecView = 'cards' | 'doc';

interface ExecSubtask {
  id: string;
  title: string;
  description: string;
  kind: 'feature' | 'screen' | 'task';
  status: SubtaskState;
  attempts: number;
  latest_implementation?: string | null;
  latest_review?: string | null;
  review_findings: string[];
  files_touched: string[];
  isolated_workspace?: string | null;
  merge_conflict?: string | null;
}

interface ExecBoard {
  task: string;
  state: BoardState;
  active_subtask_id: string | null;
  active_subtask_ids?: string[];
  subtasks: ExecSubtask[];
  updated_at: string;
}

const SUBTASK_STATUS_PRIORITY: Record<SubtaskState, number> = {
  in_progress: 0,
  needs_fix: 1,
  pending: 2,
  done: 3,
  failed: 4,
};

const STATUS_OPTIONS: Array<{ id: 'all' | SubtaskState; label: string }> = [
  { id: 'all', label: 'All' },
  { id: 'in_progress', label: 'Active' },
  { id: 'needs_fix', label: 'Needs Fix' },
  { id: 'done', label: 'Done' },
  { id: 'pending', label: 'Pending' },
  { id: 'failed', label: 'Failed' },
];

export default function BlackboardPanel({
  workspacePath,
  events,
  onClose,
  onBack,
  fullscreen = false,
  unreadAgentMessages = 0,
}: BlackboardPanelProps) {
  const [activeBoard, setActiveBoard] = useState<'plan' | 'exec'>('plan');
  const [planMd, setPlanMd] = useState<string | null>(null);
  const [execMd, setExecMd] = useState<string | null>(null);
  const [execJson, setExecJson] = useState<ExecBoard | null>(null);
  const [loading, setLoading] = useState(false);
  const [execView, setExecView] = useState<ExecView>('cards');
  const [selectedStatus, setSelectedStatus] = useState<'all' | SubtaskState>('all');
  const [selectedSubtaskId, setSelectedSubtaskId] = useState<string | null>(null);
  const [activityCollapsed, setActivityCollapsed] = useState(false);
  const [activityHeight, setActivityHeight] = useState(160);
  const autoSwitchedExecRef = useRef(false);
  const focusSubtask = useCallback((subtaskId: string) => {
    setActiveBoard('exec');
    setExecView('cards');
    setSelectedSubtaskId(subtaskId);
  }, []);

  useEffect(() => {
    autoSwitchedExecRef.current = false;
    setActiveBoard('plan');
    setExecView('cards');
    setSelectedStatus('all');
    setSelectedSubtaskId(null);
    setActivityCollapsed(false);
    setActivityHeight(160);
  }, [workspacePath]);

  const handleActivityResizeStart = useCallback((event: React.MouseEvent<HTMLDivElement>) => {
    event.preventDefault();
    const startY = event.clientY;
    const startHeight = activityHeight;

    const handleMouseMove = (moveEvent: MouseEvent) => {
      const delta = startY - moveEvent.clientY;
      const nextHeight = Math.max(108, Math.min(320, startHeight + delta));
      setActivityHeight(nextHeight);
    };

    const handleMouseUp = () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };

    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);
  }, [activityHeight]);

  const loadBoards = useCallback(async () => {
    if (!workspacePath) {
      setPlanMd(null);
      setExecMd(null);
      setExecJson(null);
      return;
    }

    setLoading(true);

    try {
      try {
        const pMd = await invoke<string>('read_workspace_file', {
          path: workspacePath,
          relativePath: '.ai-dev-hub/PLAN_BLACKBOARD.md',
        });
        setPlanMd(pMd);
      } catch {
        try {
          const pMd = await invoke<string>('read_workspace_file', {
            path: workspacePath,
            relativePath: '.ai-dev-hub/PLAN.md',
          });
          setPlanMd(pMd);
        } catch {
          setPlanMd(null);
        }
      }

      try {
        const eMd = await invoke<string>('read_workspace_file', {
          path: workspacePath,
          relativePath: '.ai-dev-hub/BLACKBOARD.md',
        });
        setExecMd(eMd);
      } catch {
        setExecMd(null);
      }

      try {
        const eJsonStr = await invoke<string>('read_workspace_file', {
          path: workspacePath,
          relativePath: '.ai-dev-hub/BLACKBOARD.json',
        });
        const parsed = JSON.parse(eJsonStr) as ExecBoard;
        if (!parsed || !Array.isArray(parsed.subtasks)) {
          throw new Error('invalid blackboard shape');
        }
        setExecJson(parsed);
        const preferredSubtaskId = parsed.active_subtask_ids?.[0] ?? parsed.active_subtask_id ?? null;
        if (preferredSubtaskId) {
          setSelectedSubtaskId(prev => prev ?? preferredSubtaskId);
        }
        if (!autoSwitchedExecRef.current) {
          setActiveBoard('exec');
          autoSwitchedExecRef.current = true;
        }
      } catch {
        setExecJson(null);
      }
    } catch (err) {
      console.error('Error loading blackboards:', err);
    } finally {
      setLoading(false);
    }
  }, [workspacePath]);

  useEffect(() => {
    loadBoards();
  }, [loadBoards, events.length]);

  const markdownComponents: Components = {
    h1: ({ children }) => (
      <h1 className="mb-3 text-[13px] font-semibold tracking-[0.02em] text-content-primary">
        {children}
      </h1>
    ),
    h2: ({ children }) => (
      <h2 className="mb-2 mt-4 border-b border-edge-primary/60 pb-1.5 text-[12px] font-semibold uppercase tracking-[0.08em] text-content-secondary">
        {children}
      </h2>
    ),
    h3: ({ children }) => (
      <h3 className="mb-2 mt-3 text-[12px] font-semibold text-content-primary">
        {children}
      </h3>
    ),
    p: ({ children }) => (
      <p className="mb-2 whitespace-pre-wrap text-[12px] leading-6 text-content-secondary">
        {children}
      </p>
    ),
    ul: ({ children }) => (
      <ul className="mb-3 space-y-1.5 pl-4 text-[12px] leading-6 text-content-secondary">
        {children}
      </ul>
    ),
    ol: ({ children }) => (
      <ol className="mb-3 space-y-1.5 pl-4 text-[12px] leading-6 text-content-secondary">
        {children}
      </ol>
    ),
    li: ({ children }) => (
      <li className="pl-1 marker:text-content-tertiary">{children}</li>
    ),
    code: ({ className, children }) =>
      !className ? (
        <code className="rounded-md bg-surface-tertiary px-1.5 py-0.5 font-mono text-[11px] text-content-secondary">
          {children}
        </code>
      ) : (
        <code className={`font-mono text-[11px] leading-5 text-content-secondary ${className}`}>
          {children}
        </code>
      ),
    pre: ({ children }) => (
      <pre className="custom-scrollbar mb-3 overflow-x-auto rounded-xl border border-edge-primary/60 bg-surface-tertiary/70 px-3 py-2.5">
        {children}
      </pre>
    ),
    blockquote: ({ children }) => (
      <blockquote className="mb-3 border-l-2 border-themed-accent/40 pl-3 text-[12px] leading-6 text-content-secondary">
        {children}
      </blockquote>
    ),
    table: ({ children }) => (
      <div className="custom-scrollbar mb-3 overflow-x-auto rounded-xl border border-edge-primary/60">
        <table className="min-w-full border-collapse text-[11px] leading-5">{children}</table>
      </div>
    ),
    th: ({ children }) => (
      <th className="border-b border-edge-primary/60 bg-surface-tertiary px-2.5 py-2 text-left font-semibold text-content-secondary">
        {children}
      </th>
    ),
    td: ({ children }) => (
      <td className="border-b border-edge-secondary px-2.5 py-2 align-top text-content-secondary">
        {children}
      </td>
    ),
    hr: () => <hr className="my-4 border-edge-primary" />,
  };

  const renderMarkdown = (content: string) => (
    <div className="max-w-none">
      <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeSanitize]} components={markdownComponents}>
        {content}
      </ReactMarkdown>
    </div>
  );

  const getStatusColor = (status: string) => {
    switch (status?.toLowerCase()) {
      case 'completed':
      case 'done':
        return 'bg-emerald-500/15 text-emerald-600 border-emerald-500/25';
      case 'in_progress':
      case 'inprogress':
        return 'bg-blue-500/15 text-blue-600 border-blue-500/25';
      case 'needs_fix':
      case 'needsfix':
        return 'bg-amber-500/15 text-amber-600 border-amber-500/25';
      case 'failed':
        return 'bg-rose-500/15 text-rose-600 border-rose-500/25';
      case 'pending':
      default:
        return 'bg-surface-tertiary text-content-secondary border-edge-primary';
    }
  };

  const getKindLabel = (kind: ExecSubtask['kind']) => {
    switch (kind) {
      case 'feature':
        return 'Feature';
      case 'screen':
        return 'Screen';
      default:
        return 'Task';
    }
  };

  const getStatusLabel = (status: string) => {
    switch (status) {
      case 'in_progress':
        return 'In Progress';
      case 'needs_fix':
        return 'Needs Fix';
      case 'done':
        return 'Done';
      case 'failed':
        return 'Failed';
      default:
        return 'Pending';
    }
  };

  const subtasks = execJson?.subtasks ?? [];
  const activeSubtaskIds = execJson?.active_subtask_ids?.length
    ? execJson.active_subtask_ids
    : execJson?.active_subtask_id
      ? [execJson.active_subtask_id]
      : [];
  const filteredSubtasks = subtasks.filter(card => {
    if (selectedStatus !== 'all' && card.status !== selectedStatus) return false;
    return true;
  });
  const sortedFilteredSubtasks = [...filteredSubtasks].sort((left, right) => {
    const leftActiveIndex = activeSubtaskIds.indexOf(left.id);
    const rightActiveIndex = activeSubtaskIds.indexOf(right.id);

    if (leftActiveIndex !== -1 || rightActiveIndex !== -1) {
      if (leftActiveIndex === -1) return 1;
      if (rightActiveIndex === -1) return -1;
      return leftActiveIndex - rightActiveIndex;
    }

    return SUBTASK_STATUS_PRIORITY[left.status] - SUBTASK_STATUS_PRIORITY[right.status]
      || left.id.localeCompare(right.id);
  });
  const focusedSubtask = subtasks.find(card => card.id === selectedSubtaskId) ?? null;
  const filteredEvents = selectedSubtaskId
    ? events.filter(ev => ev.subtask_id === selectedSubtaskId || ev.subtask_id === null)
    : events;
  const activeSubtaskCards = activeSubtaskIds
    .map(subtaskId => subtasks.find(card => card.id === subtaskId) ?? null)
    .filter((card): card is ExecSubtask => card !== null);
  const queuedSubtasks = subtasks.filter(card =>
    !activeSubtaskIds.includes(card.id) && (card.status === 'pending' || card.status === 'needs_fix')
  );
  const summary = {
    total: subtasks.length,
    done: subtasks.filter(card => card.status === 'done').length,
    active: subtasks.filter(card => card.status === 'in_progress').length,
    needsFix: subtasks.filter(card => card.status === 'needs_fix').length,
  };

  useEffect(() => {
    if (activeBoard !== 'exec' || execView !== 'cards') return;

    if (sortedFilteredSubtasks.length === 0) {
      if (selectedSubtaskId !== null) {
        setSelectedSubtaskId(null);
      }
      return;
    }

    const hasVisibleSelection = selectedSubtaskId
      ? sortedFilteredSubtasks.some(card => card.id === selectedSubtaskId)
      : false;

    if (!hasVisibleSelection) {
      const preferredActive = activeSubtaskIds.find(subtaskId =>
        sortedFilteredSubtasks.some(card => card.id === subtaskId)
      );
      setSelectedSubtaskId(preferredActive ?? sortedFilteredSubtasks[0].id);
    }
  }, [activeBoard, execView, sortedFilteredSubtasks, selectedSubtaskId, activeSubtaskIds]);

  return (
    <div className="flex h-full flex-col bg-transparent font-sans">
      <div className="flex shrink-0 items-center justify-between border-b border-edge-primary/30 px-4 py-3 backdrop-blur-md bg-surface-secondary/40">
        <div className="flex items-center gap-3">
          {fullscreen && onBack && (
            <button
              onClick={onBack}
              className="flex items-center gap-2 rounded-xl border border-edge-primary/50 bg-surface-elevated/70 px-3 py-2 text-[11px] font-medium text-content-primary transition-colors hover:bg-surface-elevated"
              title="Back to chat workspace"
            >
              <VscArrowLeft className="h-4 w-4" />
              <span>Back</span>
              {unreadAgentMessages > 0 && (
                <span className="rounded-full bg-blue-500 px-1.5 py-0.5 text-[10px] font-semibold text-white">
                  {unreadAgentMessages}
                </span>
              )}
            </button>
          )}
          <div>
          <h2 className="flex items-center gap-2 text-sm font-semibold text-content-primary">
            <span className="flex h-6 w-6 items-center justify-center rounded-lg bg-themed-accent/10 text-themed-accent-text">
              <VscChecklist className="h-3.5 w-3.5" />
            </span>
            BLACKBOARD
          </h2>
          <p className="mt-0.5 max-w-[220px] truncate text-[11px] text-zinc-500" title={workspacePath || 'No workspace'}>
            {workspacePath ? workspacePath.split('/').pop() : 'No active workspace'}
          </p>
          </div>
        </div>
        <div className="flex items-center gap-1">
          {fullscreen && unreadAgentMessages > 0 && (
            <div className="rounded-full border border-blue-500/25 bg-blue-500/10 px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-blue-600">
              {unreadAgentMessages} new agent update{unreadAgentMessages > 1 ? 's' : ''}
            </div>
          )}
          <button
            onClick={loadBoards}
            className={`rounded-md p-1.5 text-content-tertiary transition-colors hover:bg-surface-tertiary/50 hover:text-content-secondary ${loading ? 'animate-spin' : ''}`}
            title="Refresh Boards"
          >
            <VscRefresh className="h-4 w-4" />
          </button>
          {!fullscreen && (
            <button
              onClick={onClose}
              className="rounded-md p-1.5 text-content-tertiary transition-colors hover:bg-surface-tertiary/50 hover:text-content-secondary"
              title="Close Panel"
            >
              <VscLayoutSidebarRightOff className="h-4 w-4" />
            </button>
          )}
        </div>
      </div>

      {workspacePath && (
        <div className="flex shrink-0 gap-1 border-b border-edge-primary/30 bg-surface-secondary/10 p-1.5">
          <button
            onClick={() => setActiveBoard('plan')}
            className={`flex-1 rounded-lg px-3 py-1.5 text-xs font-medium transition-all ${activeBoard === 'plan'
              ? 'bg-surface-elevated/80 text-content-primary shadow-sm'
              : 'text-content-tertiary hover:bg-surface-elevated/40 hover:text-content-primary'}`}
          >
            Plan Board
            {planMd && <span className="ml-1.5 inline-block h-1.5 w-1.5 rounded-full bg-emerald-400" />}
          </button>
          <button
            onClick={() => setActiveBoard('exec')}
            className={`flex-1 rounded-lg px-3 py-1.5 text-xs font-medium transition-all ${activeBoard === 'exec'
              ? 'bg-surface-elevated/80 text-content-primary shadow-sm'
              : 'text-content-tertiary hover:bg-surface-elevated/40 hover:text-content-primary'}`}
          >
            Exec Board
            {execMd && <span className="ml-1.5 inline-block h-1.5 w-1.5 rounded-full bg-blue-400" />}
          </button>
        </div>
      )}

      <div className="custom-scrollbar flex-1 overflow-y-auto p-4">
        {!workspacePath ? (
          <div className="flex h-full flex-col items-center justify-center px-4 text-center">
            <div className="mb-3 flex h-12 w-12 items-center justify-center rounded-full bg-surface-tertiary">
              <VscChecklist className="h-6 w-6 text-content-tertiary" />
            </div>
            <p className="mb-1 text-sm font-medium text-content-secondary">No Workspace Active</p>
            <p className="text-xs text-content-tertiary">
              Open a project to view its planning and execution blackboards.
            </p>
          </div>
        ) : activeBoard === 'plan' ? (
          <div className="space-y-3">
            <div className="border border-edge-primary/40 bg-surface-elevated/50 px-4 py-3 rounded-2xl">
              <div className="mb-2 flex items-center justify-between">
                <span className="text-[10px] font-semibold uppercase tracking-[0.12em] text-content-tertiary">Mission Status</span>
                {execJson?.state && (
                  <span className={`rounded-full border px-2 py-0.5 text-[10px] ${getStatusColor(execJson.state)}`}>
                    {execJson.state.replace('_', ' ').toUpperCase()}
                  </span>
                )}
              </div>
              {activeSubtaskIds.length > 0 ? (
                <div className="rounded-xl border border-blue-500/20 bg-blue-500/8 p-3">
                  <div className="mb-2 flex items-center gap-2">
                    <span className="h-2 w-2 animate-pulse rounded-full bg-blue-500" />
                    <span className="text-[11px] font-semibold text-blue-600">
                      Active Lanes: {activeSubtaskIds.length}
                    </span>
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {activeSubtaskIds.map(activeId => (
                      <button
                        key={activeId}
                        onClick={() => focusSubtask(activeId)}
                        className="rounded-full border border-blue-500/20 bg-surface-elevated/80 px-2 py-1 text-[10px] font-medium text-blue-600 transition-colors hover:bg-surface-elevated"
                      >
                        {activeId}
                      </button>
                    ))}
                  </div>
                </div>
              ) : (
                <div className="rounded-xl border border-edge-primary/50 bg-surface-tertiary/50 p-3">
                  <span className="text-[11px] leading-5 text-content-tertiary">
                    {planMd ? 'Plan active. Waiting for code execution to start.' : 'No active tasks.'}
                  </span>
                </div>
              )}
            </div>

            {planMd ? (
              <div className="rounded-2xl border border-edge-primary/30 bg-surface-elevated/40 overflow-hidden p-3.5">
                {renderMarkdown(planMd)}
              </div>
            ) : (
              <div className="py-8 text-center">
                <p className="text-[11px] text-zinc-500">No planning board found (PLAN.md)</p>
              </div>
            )}
          </div>
        ) : execMd ? (
          <div className="space-y-3">
            <div className="flex items-center justify-between gap-2 rounded-2xl border border-edge-primary/40 bg-surface-elevated/50 px-3 py-2">
              <div>
                <div className="text-[10px] font-semibold uppercase tracking-[0.12em] text-content-tertiary">Execution View</div>
                <div className="mt-0.5 text-[11px] text-content-tertiary">
                  {execJson ? 'Structured board with inline review state.' : 'Markdown execution log.'}
                </div>
              </div>
              <div className="flex items-center gap-1 rounded-lg bg-surface-tertiary/80 p-1">
                <button
                  onClick={() => setExecView('cards')}
                  className={`rounded-md px-2.5 py-1 text-[11px] font-medium transition-colors ${execView === 'cards' ? 'bg-surface-elevated text-content-primary shadow-sm' : 'text-content-tertiary hover:text-content-primary'}`}
                >
                  Cards
                </button>
                <button
                  onClick={() => setExecView('doc')}
                  className={`rounded-md px-2.5 py-1 text-[11px] font-medium transition-colors ${execView === 'doc' ? 'bg-surface-elevated text-content-primary shadow-sm' : 'text-content-tertiary hover:text-content-primary'}`}
                >
                  Document
                </button>
              </div>
            </div>

            {execView === 'cards' ? (
              execJson ? (
                <>
                  {/* Progress bar */}
                  {summary.total > 0 && (
                    <div className="rounded-xl border border-edge-primary/30 bg-surface-elevated/50 px-4 py-3">
                      <div className="mb-2 flex items-center justify-between">
                        <span className="text-[10px] font-semibold uppercase tracking-[0.1em] text-zinc-500">Overall Progress</span>
                        <span className="text-[12px] font-bold tabular-nums text-themed-accent-text">
                          {Math.round((summary.done / summary.total) * 100)}%
                        </span>
                      </div>
                      <div className="h-1.5 rounded-full overflow-hidden bg-surface-tertiary/60">
                        <div
                          className="h-full rounded-full bg-violet-500 transition-all duration-700 ease-out"
                          style={{ width: `${Math.round((summary.done / summary.total) * 100)}%` }}
                        />
                      </div>
                    </div>
                  )}

                  <div className="grid grid-cols-4 gap-1.5">
                    {[
                      { value: summary.total, label: 'Total', border: 'border-edge-primary/40', bg: 'bg-surface-elevated/60', valColor: 'text-content-primary', labelColor: 'text-zinc-400' },
                      { value: summary.done, label: 'Done', border: 'border-emerald-500/20', bg: 'bg-emerald-500/8', valColor: 'text-emerald-600', labelColor: 'text-emerald-500/80' },
                      { value: summary.active, label: 'Active', border: 'border-blue-500/20', bg: 'bg-blue-500/8', valColor: 'text-blue-600', labelColor: 'text-blue-500/80' },
                      { value: summary.needsFix, label: 'Fix', border: 'border-amber-500/20', bg: 'bg-amber-500/8', valColor: 'text-amber-600', labelColor: 'text-amber-500/80' },
                    ].map(s => (
                      <div key={s.label} className={`rounded-xl border ${s.border} ${s.bg} px-3 py-2.5 transition-all`}>
                        <div className={`text-xl font-bold tabular-nums tracking-tight ${s.valColor}`}>{s.value}</div>
                        <div className={`text-[10px] font-semibold uppercase tracking-[0.1em] ${s.labelColor}`}>{s.label}</div>
                      </div>
                    ))}
                  </div>

                  <div className="flex flex-wrap gap-1.5">
                    {STATUS_OPTIONS.map(option => {
                      const count = option.id === 'all'
                        ? subtasks.length
                        : subtasks.filter(card => card.status === option.id).length;
                      return (
                        <button
                          key={option.id}
                          onClick={() => setSelectedStatus(option.id)}
                          className={`rounded-full border px-2.5 py-1 text-[10px] font-medium transition-colors ${selectedStatus === option.id
                            ? 'border-themed-accent/30 bg-themed-accent-soft text-themed-accent-text'
                            : 'border-edge-primary bg-surface-elevated/70 text-content-tertiary hover:text-content-primary'}`}
                        >
                          {option.label} {count}
                        </button>
                      );
                    })}
                  </div>

                  <div className="rounded-2xl border border-edge-primary/30 bg-surface-elevated/40 p-3">
                    <div className="flex flex-wrap items-start justify-between gap-3">
                      <div>
                        <div className="text-[10px] font-semibold uppercase tracking-[0.12em] text-content-tertiary">
                          Parallel Lanes
                        </div>
                        <div className="mt-1 text-[11px] leading-5 text-content-tertiary">
                          Each lane runs implementation in an isolated workspace, then reviews before merge.
                        </div>
                      </div>
                      <div className="flex flex-wrap gap-1.5 text-[10px]">
                        <span className="rounded-full border border-blue-500/20 bg-blue-500/10 px-2 py-1 font-medium text-blue-600">
                          Running {activeSubtaskCards.length}
                        </span>
                        <span className="rounded-full border border-amber-500/20 bg-amber-500/10 px-2 py-1 font-medium text-amber-600">
                          Queue {queuedSubtasks.length}
                        </span>
                        <span className="rounded-full border border-emerald-500/20 bg-emerald-500/10 px-2 py-1 font-medium text-emerald-600">
                          Merged {summary.done}
                        </span>
                      </div>
                    </div>

                    {activeSubtaskCards.length > 0 ? (
                      <div className="mt-3 grid gap-2 md:grid-cols-2 2xl:grid-cols-3">
                        {activeSubtaskCards.map(card => (
                          <button
                            key={card.id}
                            onClick={() => focusSubtask(card.id)}
                            className={`rounded-2xl border px-3 py-3 text-left transition-all ${selectedSubtaskId === card.id
                              ? 'border-blue-500/35 bg-blue-500/10 shadow-sm'
                              : 'border-edge-primary/60 bg-surface-elevated/80 hover:border-blue-400/30 hover:bg-surface-elevated'}`}
                          >
                            <div className="flex items-start justify-between gap-3">
                              <div className="flex items-center gap-2">
                                <span className="h-2.5 w-2.5 shrink-0 animate-pulse rounded-full bg-blue-500" />
                                <span className="rounded-md bg-surface-elevated/90 px-1.5 py-0.5 font-mono text-[10px] text-content-secondary shadow-sm">
                                  {card.id}
                                </span>
                              </div>
                              <span className={`shrink-0 rounded-full border px-2 py-0.5 text-[10px] font-medium ${getStatusColor(card.status)}`}>
                                {getStatusLabel(card.status)}
                              </span>
                            </div>
                            <div className="mt-2 text-[12px] font-semibold leading-5 text-content-primary">
                              {card.title}
                            </div>
                            <div className="mt-1 line-clamp-3 text-[11px] leading-5 text-content-secondary">
                              {card.description}
                            </div>
                            <div className="mt-3 flex flex-wrap gap-1.5 text-[10px]">
                              <span className="rounded-full bg-surface-elevated/85 px-2 py-1 text-content-secondary">
                                Attempt {card.attempts}
                              </span>
                              {card.files_touched.length > 0 && (
                                <span className="rounded-full bg-surface-elevated/85 px-2 py-1 text-content-secondary">
                                  Files {card.files_touched.length}
                                </span>
                              )}
                              {card.review_findings.length > 0 && (
                                <span className="rounded-full bg-amber-500/10 px-2 py-1 text-amber-600">
                                  Findings {card.review_findings.length}
                                </span>
                              )}
                              {card.isolated_workspace && (
                                <span className="rounded-full bg-sky-500/10 px-2 py-1 text-sky-600">
                                  Isolated
                                </span>
                              )}
                            </div>
                          </button>
                        ))}
                      </div>
                    ) : (
                      <div className="mt-3 rounded-2xl border border-edge-primary/60 bg-surface-elevated/70 px-3 py-4 text-[11px] leading-5 text-content-tertiary">
                        No lane is running right now. The queue and completed subtasks remain available below for inspection.
                      </div>
                    )}

                    {queuedSubtasks.length > 0 && (
                      <div className="mt-3 rounded-2xl border border-edge-primary/50 bg-surface-elevated/60 px-3 py-3">
                        <div className="mb-2 text-[10px] font-semibold uppercase tracking-[0.12em] text-content-tertiary">
                          Up Next
                        </div>
                        <div className="flex flex-wrap gap-1.5">
                          {queuedSubtasks.slice(0, 8).map(card => (
                            <button
                              key={card.id}
                              onClick={() => focusSubtask(card.id)}
                              className="rounded-full border border-edge-primary bg-surface-elevated/85 px-2.5 py-1 text-[10px] font-medium text-content-secondary transition-colors hover:text-content-primary"
                            >
                              {card.id} · {card.title}
                            </button>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>

                  <div className="grid min-h-[30rem] gap-3 xl:grid-cols-[minmax(320px,0.92fr)_minmax(0,1.4fr)]">
                    <div className="rounded-2xl border border-edge-primary/30 bg-surface-elevated/40 custom-scrollbar overflow-y-auto p-2.5">
                      <div className="mb-2 flex items-center justify-between gap-2 px-1">
                        <div className="text-[10px] font-semibold uppercase tracking-[0.12em] text-content-tertiary">
                          Board Queue
                        </div>
                        <div className="text-[10px] text-content-tertiary">
                          {sortedFilteredSubtasks.length}/{subtasks.length}
                        </div>
                      </div>
                      <div className="space-y-2">
                        {sortedFilteredSubtasks.length > 0 ? (
                          sortedFilteredSubtasks.map(card => (
                            <button
                              key={card.id}
                              onClick={() => setSelectedSubtaskId(card.id)}
                              className={`w-full rounded-xl border px-3 py-2.5 text-left transition-all ${selectedSubtaskId === card.id
                                ? 'border-themed-accent/30 bg-themed-accent-soft/80 shadow-sm'
                                : 'border-edge-primary/60 bg-surface-elevated/70 hover:border-edge-primary hover:bg-surface-elevated'}`}
                            >
                              <div className="mb-1 flex items-center justify-between gap-2">
                                <div className="flex items-center gap-2">
                                  {activeSubtaskIds.includes(card.id) && (
                                    <span className="h-2 w-2 animate-pulse rounded-full bg-blue-500" />
                                  )}
                                  <span className="rounded-md bg-surface-tertiary px-1.5 py-0.5 font-mono text-[10px] text-content-secondary">
                                    {card.id}
                                  </span>
                                </div>
                                <span className={`shrink-0 rounded-full border px-2 py-0.5 text-[10px] font-medium ${getStatusColor(card.status)}`}>
                                  {getStatusLabel(card.status)}
                                </span>
                              </div>
                              <div className="text-[11px] font-semibold leading-5 text-content-primary">{card.title}</div>
                              <div className="mt-1 line-clamp-2 text-[10px] leading-4 text-content-tertiary">{card.description}</div>
                              <div className="mt-2 flex flex-wrap gap-1.5 text-[10px] text-content-tertiary">
                                <span className="rounded-full bg-surface-tertiary px-2 py-0.5">
                                  Attempt {card.attempts}
                                </span>
                                {card.review_findings.length > 0 && (
                                  <span className="rounded-full bg-amber-500/10 px-2 py-0.5 text-amber-600">
                                    Findings {card.review_findings.length}
                                  </span>
                                )}
                                {card.files_touched.length > 0 && (
                                  <span className="rounded-full bg-surface-tertiary px-2 py-0.5">
                                    Files {card.files_touched.length}
                                  </span>
                                )}
                              </div>
                            </button>
                          ))
                        ) : (
                          <div className="rounded-xl border border-edge-primary/60 bg-surface-elevated/70 px-3 py-4 text-center text-[11px] text-content-tertiary">
                            No subtasks match the current filter.
                          </div>
                        )}
                      </div>
                    </div>

                    <div className="rounded-2xl border border-edge-primary/30 bg-surface-elevated/40 p-3.5">
                      {focusedSubtask ? (
                        <div className="space-y-4">
                          <div className="flex items-start justify-between gap-3">
                            <div>
                              <div className="mb-2 flex items-center gap-2">
                                <span className="rounded-md bg-surface-tertiary px-1.5 py-0.5 font-mono text-[10px] text-content-secondary">
                                  {focusedSubtask.id}
                                </span>
                                <span className="rounded-full border border-edge-primary px-2 py-0.5 text-[10px] text-content-tertiary">
                                  {getKindLabel(focusedSubtask.kind)}
                                </span>
                              </div>
                              <h3 className="text-[15px] font-semibold leading-6 text-content-primary">{focusedSubtask.title}</h3>
                              <p className="mt-2 text-[12px] leading-6 text-content-secondary">{focusedSubtask.description}</p>
                            </div>
                            <span className={`shrink-0 rounded-full border px-2.5 py-1 text-[10px] font-medium ${getStatusColor(focusedSubtask.status)}`}>
                              {getStatusLabel(focusedSubtask.status)}
                            </span>
                          </div>

                          <div className="flex flex-wrap gap-1.5 text-[10px] text-content-tertiary">
                            <span className="rounded-full bg-surface-tertiary px-2 py-1">Attempts {focusedSubtask.attempts}</span>
                            {focusedSubtask.files_touched.length > 0 && (
                              <span className="rounded-full bg-surface-tertiary px-2 py-1">
                                Files {focusedSubtask.files_touched.length}
                              </span>
                            )}
                          {focusedSubtask.review_findings.length > 0 && (
                            <span className="rounded-full bg-amber-500/10 px-2 py-1 text-amber-600">
                              Findings {focusedSubtask.review_findings.length}
                            </span>
                          )}
                          {focusedSubtask.isolated_workspace && (
                            <span className="rounded-full bg-sky-500/10 px-2 py-1 text-sky-600">
                              Isolated Run
                            </span>
                          )}
                          <button
                            onClick={() => setSelectedSubtaskId(null)}
                            className="rounded-full border border-edge-primary bg-surface-elevated/70 px-2 py-1 text-[10px] font-medium text-content-tertiary transition-colors hover:text-content-primary"
                          >
                              Follow Live Lane
                            </button>
                          </div>

                          {focusedSubtask.latest_implementation && (
                            <div>
                              <div className="mb-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-content-tertiary">
                                Latest Implementation
                              </div>
                              <div className="rounded-xl border border-edge-primary/60 bg-surface-elevated/70 px-3 py-2.5 text-[11px] leading-5 text-content-secondary">
                                {focusedSubtask.latest_implementation}
                              </div>
                            </div>
                          )}

                          {focusedSubtask.latest_review && (
                            <div>
                              <div className="mb-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-content-tertiary">
                                Latest Review
                              </div>
                              <div className="rounded-xl border border-edge-primary/60 bg-surface-elevated/70 px-3 py-2.5 text-[11px] leading-5 text-content-secondary">
                                {focusedSubtask.latest_review}
                              </div>
                            </div>
                          )}

                          {focusedSubtask.merge_conflict && (
                            <div>
                              <div className="mb-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-content-tertiary">
                                Merge Conflict
                              </div>
                              <div className="rounded-xl border border-rose-500/20 bg-rose-500/10 px-3 py-2.5 text-[11px] leading-5 text-rose-600">
                                {focusedSubtask.merge_conflict}
                              </div>
                            </div>
                          )}

                          {focusedSubtask.review_findings.length > 0 && (
                            <div>
                              <div className="mb-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-content-tertiary">
                                Findings
                              </div>
                              <ul className="space-y-2 text-[11px] leading-5 text-content-secondary">
                                {focusedSubtask.review_findings.map((finding, idx) => (
                                  <li key={idx} className="rounded-xl bg-surface-tertiary/80 px-3 py-2.5">
                                    {finding}
                                  </li>
                                ))}
                              </ul>
                            </div>
                          )}

                          {focusedSubtask.files_touched.length > 0 && (
                            <div>
                              <div className="mb-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-content-tertiary">
                                Touched Files
                              </div>
                              <div className="flex flex-wrap gap-1.5">
                                {focusedSubtask.files_touched.map(file => (
                                  <span key={file} className="rounded-md bg-surface-tertiary px-2 py-1 font-mono text-[10px] text-content-secondary">
                                    {file}
                                  </span>
                                ))}
                              </div>
                            </div>
                          )}

                          {focusedSubtask.isolated_workspace && (
                            <div>
                              <div className="mb-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-content-tertiary">
                                Isolated Workspace
                              </div>
                              <div className="rounded-xl border border-edge-primary/60 bg-surface-elevated/70 px-3 py-2.5 font-mono text-[11px] leading-5 text-content-secondary">
                                {focusedSubtask.isolated_workspace}
                              </div>
                            </div>
                          )}
                        </div>
                      ) : (
                        <div className="flex h-full flex-col items-center justify-center text-center">
                          <p className="text-[12px] font-medium text-content-secondary">No visible subtask selected</p>
                          <p className="mt-1 max-w-sm text-[11px] leading-5 text-content-tertiary">
                            Adjust the status filter or pick a subtask from the list to inspect its implementation summary and review findings.
                          </p>
                        </div>
                      )}
                    </div>
                  </div>
                </>
              ) : (
                <div className="rounded-2xl border border-edge-primary/30 bg-surface-elevated/40 p-4">
                  <p className="text-[12px] font-medium text-content-secondary">
                    Structured execution data is not available yet.
                  </p>
                  <p className="mt-1 text-[11px] leading-5 text-content-tertiary">
                    Switch to Document view to inspect the raw execution board, or rerun the code flow to regenerate `BLACKBOARD.json`.
                  </p>
                </div>
              )
            ) : (
              <div className="rounded-2xl border border-edge-primary/30 bg-surface-elevated/40 overflow-hidden p-3.5">
                {renderMarkdown(execMd)}
              </div>
            )}
          </div>
        ) : (
          <div className="py-8 text-center">
            <p className="text-[11px] text-zinc-500">No execution board found (BLACKBOARD.md)</p>
            <p className="mt-2 text-[10px] text-zinc-400">Run the 'code' skill to generate it.</p>
          </div>
        )}
      </div>

      {events.length > 0 && (
        <div
          className="flex shrink-0 flex-col border-t border-edge-primary/30 bg-surface-secondary/20 backdrop-blur-sm"
          style={{ height: activityCollapsed ? 'auto' : `${activityHeight}px` }}
        >
          {!activityCollapsed && (
            <div
              onMouseDown={handleActivityResizeStart}
              className="flex h-3 cursor-row-resize items-center justify-center border-b border-edge-primary/20 bg-surface-secondary/10"
              title="Drag to resize"
            >
              <div className="h-1 w-10 rounded-full bg-content-tertiary/40" />
            </div>
          )}
          <div className="flex shrink-0 items-center justify-between gap-3 border-b border-edge-primary/30 bg-surface-tertiary/30 px-3 py-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-content-secondary backdrop-blur-sm">
            <div className="flex items-center gap-2">
              <VscChevronRight className="h-3.5 w-3.5" />
              <span>Recent Activity</span>
              <span className="rounded-full bg-surface-tertiary/80 px-1.5 py-0.5 text-[10px]">
                {Math.min(filteredEvents.length, 10)}
              </span>
            </div>
            <button
              onClick={() => setActivityCollapsed(prev => !prev)}
              className="rounded-md p-1 text-content-tertiary transition-colors hover:bg-surface-tertiary/60 hover:text-content-primary"
              title={activityCollapsed ? 'Expand activity' : 'Collapse activity'}
            >
              {activityCollapsed ? <VscChevronUp className="h-4 w-4" /> : <VscChevronDown className="h-4 w-4" />}
            </button>
          </div>
          {!activityCollapsed && (
            <div className="custom-scrollbar flex flex-1 flex-col gap-0 overflow-y-auto px-3 py-2">
              {[...filteredEvents].reverse().slice(0, 10).map((ev, i, arr) => (
                  <button
                    key={i}
                    onClick={() => {
                      if (ev.subtask_id) {
                        focusSubtask(ev.subtask_id);
                      }
                    }}
                  className="group/ev relative flex gap-3 py-2 text-left transition-colors hover:bg-surface-tertiary/40 rounded-lg px-1"
                >
                  {/* Timeline line */}
                  {i < arr.length - 1 && (
                    <div className="absolute left-[9px] top-[28px] bottom-0 w-[1.5px] bg-gradient-to-b from-edge-primary to-edge-secondary/50" />
                  )}

                  {/* Timeline dot */}
                  <div className={`timeline-dot mt-0.5 shrink-0 ${
                    ev.status === 'completed' || ev.status === 'done'
                      ? 'success'
                      : ev.status === 'failed'
                        ? 'error'
                        : ev.status === 'needs_fix'
                          ? 'warning'
                          : 'info'
                  }`} />

                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 flex-wrap">
                      {ev.subtask_id && (
                        <span className="rounded-md bg-surface-tertiary px-1.5 py-0.5 font-mono text-[10px] font-medium text-content-secondary">
                          {ev.subtask_id}
                        </span>
                      )}
                      <span className="text-[11px] font-medium leading-5 text-content-secondary group-hover/ev:text-content-primary transition-colors">
                        {ev.summary}
                      </span>
                    </div>
                  </div>
                </button>
              ))}
            </div>
          )}
          {activityCollapsed && (
            <div className="px-3 py-2 text-[11px] text-content-tertiary">
              Activity timeline is collapsed.
            </div>
          )}
        </div>
      )}
    </div>
  );
}
