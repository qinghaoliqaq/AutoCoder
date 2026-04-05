import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { VscArrowLeft, VscChevronDown, VscChevronRight, VscChevronUp, VscLayoutSidebarRightOff, VscRefresh } from 'react-icons/vsc';
import { BlackboardEvent } from '../types';
import ReactMarkdown, { Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';

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
        await invoke('sanitize_blackboard_state', { path: workspacePath });
      } catch (err) {
        console.warn('Error sanitizing blackboard state:', err);
      }

      try {
        const pMd = await invoke<string>('read_workspace_file', {
          path: workspacePath,
          relativePath: 'PLAN_BLACKBOARD.md',
        });
        setPlanMd(pMd);
      } catch {
        try {
          const pMd = await invoke<string>('read_workspace_file', {
            path: workspacePath,
            relativePath: 'PLAN.md',
          });
          setPlanMd(pMd);
        } catch {
          setPlanMd(null);
        }
      }

      try {
        const eMd = await invoke<string>('read_workspace_file', {
          path: workspacePath,
          relativePath: 'BLACKBOARD.md',
        });
        setExecMd(eMd);
      } catch {
        setExecMd(null);
      }

      try {
        const eJsonStr = await invoke<string>('read_workspace_file', {
          path: workspacePath,
          relativePath: 'BLACKBOARD.json',
        });
        const parsed = JSON.parse(eJsonStr) as ExecBoard;
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
      <h1 className="mb-3 text-[13px] font-semibold tracking-[0.02em] text-zinc-900 dark:text-zinc-100">
        {children}
      </h1>
    ),
    h2: ({ children }) => (
      <h2 className="mb-2 mt-4 border-b border-zinc-200/80 pb-1.5 text-[12px] font-semibold uppercase tracking-[0.08em] text-zinc-600 dark:border-zinc-800 dark:text-zinc-300">
        {children}
      </h2>
    ),
    h3: ({ children }) => (
      <h3 className="mb-2 mt-3 text-[12px] font-semibold text-zinc-800 dark:text-zinc-200">
        {children}
      </h3>
    ),
    p: ({ children }) => (
      <p className="mb-2 whitespace-pre-wrap text-[12px] leading-6 text-zinc-700 dark:text-zinc-300">
        {children}
      </p>
    ),
    ul: ({ children }) => (
      <ul className="mb-3 space-y-1.5 pl-4 text-[12px] leading-6 text-zinc-700 dark:text-zinc-300">
        {children}
      </ul>
    ),
    ol: ({ children }) => (
      <ol className="mb-3 space-y-1.5 pl-4 text-[12px] leading-6 text-zinc-700 dark:text-zinc-300">
        {children}
      </ol>
    ),
    li: ({ children }) => (
      <li className="pl-1 marker:text-zinc-400 dark:marker:text-zinc-500">{children}</li>
    ),
    code: ({ className, children }) =>
      !className ? (
        <code className="rounded-md bg-zinc-100 px-1.5 py-0.5 font-mono text-[11px] text-zinc-700 dark:bg-zinc-800 dark:text-zinc-200">
          {children}
        </code>
      ) : (
        <code className={`font-mono text-[11px] leading-5 text-zinc-700 dark:text-zinc-200 ${className}`}>
          {children}
        </code>
      ),
    pre: ({ children }) => (
      <pre className="custom-scrollbar mb-3 overflow-x-auto rounded-xl border border-zinc-200/80 bg-zinc-50 px-3 py-2.5 dark:border-zinc-800 dark:bg-zinc-950/70">
        {children}
      </pre>
    ),
    blockquote: ({ children }) => (
      <blockquote className="mb-3 border-l-2 border-violet-300 pl-3 text-[12px] leading-6 text-zinc-600 dark:border-violet-500/40 dark:text-zinc-300">
        {children}
      </blockquote>
    ),
    table: ({ children }) => (
      <div className="custom-scrollbar mb-3 overflow-x-auto rounded-xl border border-zinc-200/80 dark:border-zinc-800">
        <table className="min-w-full border-collapse text-[11px] leading-5">{children}</table>
      </div>
    ),
    th: ({ children }) => (
      <th className="border-b border-zinc-200 bg-zinc-100 px-2.5 py-2 text-left font-semibold text-zinc-600 dark:border-zinc-800 dark:bg-zinc-900 dark:text-zinc-300">
        {children}
      </th>
    ),
    td: ({ children }) => (
      <td className="border-b border-zinc-100 px-2.5 py-2 align-top text-zinc-700 dark:border-zinc-900 dark:text-zinc-300">
        {children}
      </td>
    ),
    hr: () => <hr className="my-4 border-zinc-200 dark:border-zinc-800" />,
  };

  const renderMarkdown = (content: string) => (
    <div className="max-w-none">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>
        {content}
      </ReactMarkdown>
    </div>
  );

  const getStatusColor = (status: string) => {
    switch (status?.toLowerCase()) {
      case 'completed':
      case 'done':
        return 'bg-emerald-100 text-emerald-700 dark:bg-emerald-500/20 dark:text-emerald-400 border-emerald-200 dark:border-emerald-500/30';
      case 'in_progress':
      case 'inprogress':
        return 'bg-blue-100 text-blue-700 dark:bg-blue-500/20 dark:text-blue-400 border-blue-200 dark:border-blue-500/30';
      case 'needs_fix':
      case 'needsfix':
        return 'bg-amber-100 text-amber-700 dark:bg-amber-500/20 dark:text-amber-400 border-amber-200 dark:border-amber-500/30';
      case 'failed':
        return 'bg-rose-100 text-rose-700 dark:bg-rose-500/20 dark:text-rose-400 border-rose-200 dark:border-rose-500/30';
      case 'pending':
      default:
        return 'bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400 border-zinc-200 dark:border-zinc-700';
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
      <div className="glass-header flex shrink-0 items-center justify-between border-b border-zinc-200/50 px-4 py-3 dark:border-zinc-800/50">
        <div className="flex items-center gap-3">
          {fullscreen && onBack && (
            <button
              onClick={onBack}
              className="flex items-center gap-2 rounded-xl border border-zinc-200/70 bg-white/70 px-3 py-2 text-[11px] font-medium text-zinc-700 transition-colors hover:bg-white dark:border-zinc-700/60 dark:bg-zinc-900/60 dark:text-zinc-200 dark:hover:bg-zinc-900"
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
          <h2 className="flex items-center gap-2 text-sm font-semibold text-zinc-800 dark:text-zinc-200">
            <span className="text-violet-500">📋</span> BLACKBOARD
          </h2>
          <p className="mt-0.5 max-w-[220px] truncate text-[11px] text-zinc-500" title={workspacePath || 'No workspace'}>
            {workspacePath ? workspacePath.split('/').pop() : 'No active workspace'}
          </p>
          </div>
        </div>
        <div className="flex items-center gap-1">
          {fullscreen && unreadAgentMessages > 0 && (
            <div className="rounded-full border border-blue-200 bg-blue-50 px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-blue-700 dark:border-blue-500/30 dark:bg-blue-500/10 dark:text-blue-300">
              {unreadAgentMessages} new agent update{unreadAgentMessages > 1 ? 's' : ''}
            </div>
          )}
          <button
            onClick={loadBoards}
            className={`rounded-md p-1.5 text-zinc-400 transition-colors hover:bg-zinc-200/50 hover:text-zinc-600 dark:hover:bg-zinc-800/50 dark:hover:text-zinc-300 ${loading ? 'animate-spin' : ''}`}
            title="Refresh Boards"
          >
            <VscRefresh className="h-4 w-4" />
          </button>
          {!fullscreen && (
            <button
              onClick={onClose}
              className="rounded-md p-1.5 text-zinc-400 transition-colors hover:bg-zinc-200/50 hover:text-zinc-600 dark:hover:bg-zinc-800/50 dark:hover:text-zinc-300"
              title="Close Panel"
            >
              <VscLayoutSidebarRightOff className="h-4 w-4" />
            </button>
          )}
        </div>
      </div>

      {workspacePath && (
        <div className="flex shrink-0 gap-1 border-b border-zinc-200/50 bg-white/20 p-2 backdrop-blur-sm dark:border-zinc-800/50 dark:bg-zinc-900/40">
          <button
            onClick={() => setActiveBoard('plan')}
            className={`flex-1 rounded-md border px-3 py-1.5 text-xs font-medium transition-all ${activeBoard === 'plan'
              ? 'border-white/60 bg-white/80 text-zinc-800 shadow-sm backdrop-blur-md dark:border-zinc-700/50 dark:bg-zinc-800/80 dark:text-zinc-200'
              : 'border-transparent text-zinc-500 hover:bg-white/40 hover:text-zinc-700 dark:hover:bg-zinc-800/40 dark:hover:text-zinc-300'}`}
          >
            Plan Board
            {planMd && <span className="ml-1.5 inline-block h-1.5 w-1.5 rounded-full bg-emerald-400" />}
          </button>
          <button
            onClick={() => setActiveBoard('exec')}
            className={`flex-1 rounded-md border px-3 py-1.5 text-xs font-medium transition-all ${activeBoard === 'exec'
              ? 'border-white/60 bg-white/80 text-zinc-800 shadow-sm backdrop-blur-md dark:border-zinc-700/50 dark:bg-zinc-800/80 dark:text-zinc-200'
              : 'border-transparent text-zinc-500 hover:bg-white/40 hover:text-zinc-700 dark:hover:bg-zinc-800/40 dark:hover:text-zinc-300'}`}
          >
            Exec Board
            {execMd && <span className="ml-1.5 inline-block h-1.5 w-1.5 rounded-full bg-blue-400" />}
          </button>
        </div>
      )}

      <div className="custom-scrollbar flex-1 overflow-y-auto p-4">
        {!workspacePath ? (
          <div className="flex h-full flex-col items-center justify-center px-4 text-center">
            <div className="mb-3 flex h-12 w-12 items-center justify-center rounded-full bg-zinc-100 dark:bg-zinc-800">
              <span className="text-xl">📋</span>
            </div>
            <p className="mb-1 text-sm font-medium text-zinc-700 dark:text-zinc-300">No Workspace Active</p>
            <p className="text-xs text-zinc-500 dark:text-zinc-400">
              Open a project to view its planning and execution blackboards.
            </p>
          </div>
        ) : activeBoard === 'plan' ? (
          <div className="space-y-3">
            <div className="border border-zinc-200/60 bg-white/50 px-4 py-3 dark:border-zinc-800/60 dark:bg-zinc-900/30 rounded-2xl">
              <div className="mb-2 flex items-center justify-between">
                <span className="text-[10px] font-semibold uppercase tracking-[0.12em] text-zinc-500 dark:text-zinc-400">Mission Status</span>
                {execJson?.state && (
                  <span className={`rounded-full border px-2 py-0.5 text-[10px] ${getStatusColor(execJson.state)}`}>
                    {execJson.state.replace('_', ' ').toUpperCase()}
                  </span>
                )}
              </div>
              {activeSubtaskIds.length > 0 ? (
                <div className="rounded-xl border border-blue-100 bg-blue-50/50 p-3 dark:border-blue-900/30 dark:bg-blue-900/10">
                  <div className="mb-2 flex items-center gap-2">
                    <span className="h-2 w-2 animate-pulse rounded-full bg-blue-500" />
                    <span className="text-[11px] font-semibold text-blue-700 dark:text-blue-400">
                      Active Lanes: {activeSubtaskIds.length}
                    </span>
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {activeSubtaskIds.map(activeId => (
                      <button
                        key={activeId}
                        onClick={() => focusSubtask(activeId)}
                        className="rounded-full border border-blue-200 bg-white/80 px-2 py-1 text-[10px] font-medium text-blue-700 transition-colors hover:bg-white dark:border-blue-500/20 dark:bg-blue-950/20 dark:text-blue-300"
                      >
                        {activeId}
                      </button>
                    ))}
                  </div>
                </div>
              ) : (
                <div className="rounded-xl border border-zinc-200 bg-zinc-100/50 p-3 dark:border-zinc-700/50 dark:bg-zinc-800/50">
                  <span className="text-[11px] leading-5 text-zinc-500 dark:text-zinc-400">
                    {planMd ? 'Plan active. Waiting for code execution to start.' : 'No active tasks.'}
                  </span>
                </div>
              )}
            </div>

            {planMd ? (
              <div className="glass-panel overflow-hidden p-3.5">
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
            <div className="flex items-center justify-between gap-2 rounded-2xl border border-zinc-200/60 bg-white/50 px-3 py-2 dark:border-zinc-800/60 dark:bg-zinc-900/30">
              <div>
                <div className="text-[10px] font-semibold uppercase tracking-[0.12em] text-zinc-500 dark:text-zinc-400">Execution View</div>
                <div className="mt-0.5 text-[11px] text-zinc-500 dark:text-zinc-400">
                  {execJson ? 'Structured board with inline review state.' : 'Markdown execution log.'}
                </div>
              </div>
              <div className="flex items-center gap-1 rounded-lg bg-zinc-100/80 p-1 dark:bg-zinc-900/70">
                <button
                  onClick={() => setExecView('cards')}
                  className={`rounded-md px-2.5 py-1 text-[11px] font-medium transition-colors ${execView === 'cards' ? 'bg-white text-zinc-900 shadow-sm dark:bg-zinc-800 dark:text-zinc-100' : 'text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-200'}`}
                >
                  Cards
                </button>
                <button
                  onClick={() => setExecView('doc')}
                  className={`rounded-md px-2.5 py-1 text-[11px] font-medium transition-colors ${execView === 'doc' ? 'bg-white text-zinc-900 shadow-sm dark:bg-zinc-800 dark:text-zinc-100' : 'text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-200'}`}
                >
                  Document
                </button>
              </div>
            </div>

            {execView === 'cards' ? (
              execJson ? (
                <>
                  <div className="grid grid-cols-2 gap-2">
                    <div className="rounded-xl border border-zinc-200/80 bg-white/80 px-3 py-2 dark:border-zinc-800 dark:bg-zinc-950/60">
                      <div className="text-[10px] uppercase tracking-[0.08em] text-zinc-400">Total</div>
                      <div className="mt-1 text-lg font-semibold text-zinc-900 dark:text-zinc-100">{summary.total}</div>
                    </div>
                    <div className="rounded-xl border border-emerald-200/80 bg-emerald-50/90 px-3 py-2 dark:border-emerald-500/20 dark:bg-emerald-500/10">
                      <div className="text-[10px] uppercase tracking-[0.08em] text-emerald-500">Done</div>
                      <div className="mt-1 text-lg font-semibold text-emerald-700 dark:text-emerald-300">{summary.done}</div>
                    </div>
                    <div className="rounded-xl border border-blue-200/80 bg-blue-50/90 px-3 py-2 dark:border-blue-500/20 dark:bg-blue-500/10">
                      <div className="text-[10px] uppercase tracking-[0.08em] text-blue-500">Active</div>
                      <div className="mt-1 text-lg font-semibold text-blue-700 dark:text-blue-300">{summary.active}</div>
                    </div>
                    <div className="rounded-xl border border-amber-200/80 bg-amber-50/90 px-3 py-2 dark:border-amber-500/20 dark:bg-amber-500/10">
                      <div className="text-[10px] uppercase tracking-[0.08em] text-amber-500">Needs Fix</div>
                      <div className="mt-1 text-lg font-semibold text-amber-700 dark:text-amber-300">{summary.needsFix}</div>
                    </div>
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
                            ? 'border-violet-300 bg-violet-50 text-violet-700 dark:border-violet-500/30 dark:bg-violet-500/10 dark:text-violet-300'
                            : 'border-zinc-200 bg-white/70 text-zinc-500 hover:text-zinc-700 dark:border-zinc-700 dark:bg-zinc-900/60 dark:text-zinc-400 dark:hover:text-zinc-200'}`}
                        >
                          {option.label} {count}
                        </button>
                      );
                    })}
                  </div>

                  <div className="glass-panel p-3">
                    <div className="flex flex-wrap items-start justify-between gap-3">
                      <div>
                        <div className="text-[10px] font-semibold uppercase tracking-[0.12em] text-zinc-500 dark:text-zinc-400">
                          Parallel Lanes
                        </div>
                        <div className="mt-1 text-[11px] leading-5 text-zinc-500 dark:text-zinc-400">
                          Each lane runs Claude implementation inside an isolated workspace, then hands the result to Codex for immediate review before merge.
                        </div>
                      </div>
                      <div className="flex flex-wrap gap-1.5 text-[10px]">
                        <span className="rounded-full border border-blue-200 bg-blue-50 px-2 py-1 font-medium text-blue-700 dark:border-blue-500/30 dark:bg-blue-500/10 dark:text-blue-300">
                          Running {activeSubtaskCards.length}
                        </span>
                        <span className="rounded-full border border-amber-200 bg-amber-50 px-2 py-1 font-medium text-amber-700 dark:border-amber-500/30 dark:bg-amber-500/10 dark:text-amber-300">
                          Queue {queuedSubtasks.length}
                        </span>
                        <span className="rounded-full border border-emerald-200 bg-emerald-50 px-2 py-1 font-medium text-emerald-700 dark:border-emerald-500/30 dark:bg-emerald-500/10 dark:text-emerald-300">
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
                              ? 'border-blue-300 bg-blue-50/90 shadow-sm dark:border-blue-500/40 dark:bg-blue-500/10'
                              : 'border-zinc-200/80 bg-white/80 hover:border-blue-200 hover:bg-white dark:border-zinc-800 dark:bg-zinc-950/55 dark:hover:border-blue-500/20 dark:hover:bg-zinc-950'}`}
                          >
                            <div className="flex items-start justify-between gap-3">
                              <div className="flex items-center gap-2">
                                <span className="h-2.5 w-2.5 shrink-0 animate-pulse rounded-full bg-blue-500" />
                                <span className="rounded-md bg-white/90 px-1.5 py-0.5 font-mono text-[10px] text-zinc-700 shadow-sm dark:bg-zinc-900/80 dark:text-zinc-200">
                                  {card.id}
                                </span>
                              </div>
                              <span className={`shrink-0 rounded-full border px-2 py-0.5 text-[10px] font-medium ${getStatusColor(card.status)}`}>
                                {getStatusLabel(card.status)}
                              </span>
                            </div>
                            <div className="mt-2 text-[12px] font-semibold leading-5 text-zinc-900 dark:text-zinc-100">
                              {card.title}
                            </div>
                            <div className="mt-1 line-clamp-3 text-[11px] leading-5 text-zinc-600 dark:text-zinc-400">
                              {card.description}
                            </div>
                            <div className="mt-3 flex flex-wrap gap-1.5 text-[10px]">
                              <span className="rounded-full bg-white/85 px-2 py-1 text-zinc-600 dark:bg-zinc-900/80 dark:text-zinc-300">
                                Attempt {card.attempts}
                              </span>
                              {card.files_touched.length > 0 && (
                                <span className="rounded-full bg-white/85 px-2 py-1 text-zinc-600 dark:bg-zinc-900/80 dark:text-zinc-300">
                                  Files {card.files_touched.length}
                                </span>
                              )}
                              {card.review_findings.length > 0 && (
                                <span className="rounded-full bg-amber-100 px-2 py-1 text-amber-700 dark:bg-amber-500/10 dark:text-amber-300">
                                  Findings {card.review_findings.length}
                                </span>
                              )}
                              {card.isolated_workspace && (
                                <span className="rounded-full bg-sky-100 px-2 py-1 text-sky-700 dark:bg-sky-500/10 dark:text-sky-300">
                                  Isolated
                                </span>
                              )}
                            </div>
                          </button>
                        ))}
                      </div>
                    ) : (
                      <div className="mt-3 rounded-2xl border border-zinc-200/80 bg-white/70 px-3 py-4 text-[11px] leading-5 text-zinc-500 dark:border-zinc-800 dark:bg-zinc-950/50 dark:text-zinc-400">
                        No lane is running right now. The queue and completed subtasks remain available below for inspection.
                      </div>
                    )}

                    {queuedSubtasks.length > 0 && (
                      <div className="mt-3 rounded-2xl border border-zinc-200/70 bg-white/60 px-3 py-3 dark:border-zinc-800/70 dark:bg-zinc-950/35">
                        <div className="mb-2 text-[10px] font-semibold uppercase tracking-[0.12em] text-zinc-500 dark:text-zinc-400">
                          Up Next
                        </div>
                        <div className="flex flex-wrap gap-1.5">
                          {queuedSubtasks.slice(0, 8).map(card => (
                            <button
                              key={card.id}
                              onClick={() => focusSubtask(card.id)}
                              className="rounded-full border border-zinc-200 bg-white/85 px-2.5 py-1 text-[10px] font-medium text-zinc-600 transition-colors hover:text-zinc-800 dark:border-zinc-700 dark:bg-zinc-900/70 dark:text-zinc-300 dark:hover:text-zinc-100"
                            >
                              {card.id} · {card.title}
                            </button>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>

                  <div className="grid min-h-[30rem] gap-3 xl:grid-cols-[minmax(320px,0.92fr)_minmax(0,1.4fr)]">
                    <div className="glass-panel custom-scrollbar overflow-y-auto p-2.5">
                      <div className="mb-2 flex items-center justify-between gap-2 px-1">
                        <div className="text-[10px] font-semibold uppercase tracking-[0.12em] text-zinc-500 dark:text-zinc-400">
                          Board Queue
                        </div>
                        <div className="text-[10px] text-zinc-400 dark:text-zinc-500">
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
                                ? 'border-violet-300 bg-violet-50/80 shadow-sm dark:border-violet-500/30 dark:bg-violet-500/10'
                                : 'border-zinc-200/80 bg-white/70 hover:border-zinc-300 hover:bg-white dark:border-zinc-800 dark:bg-zinc-950/50 dark:hover:border-zinc-700 dark:hover:bg-zinc-950'}`}
                            >
                              <div className="mb-1 flex items-center justify-between gap-2">
                                <div className="flex items-center gap-2">
                                  {activeSubtaskIds.includes(card.id) && (
                                    <span className="h-2 w-2 animate-pulse rounded-full bg-blue-500" />
                                  )}
                                  <span className="rounded-md bg-zinc-100 px-1.5 py-0.5 font-mono text-[10px] text-zinc-700 dark:bg-zinc-800 dark:text-zinc-200">
                                    {card.id}
                                  </span>
                                </div>
                                <span className={`shrink-0 rounded-full border px-2 py-0.5 text-[10px] font-medium ${getStatusColor(card.status)}`}>
                                  {getStatusLabel(card.status)}
                                </span>
                              </div>
                              <div className="text-[11px] font-semibold leading-5 text-zinc-900 dark:text-zinc-100">{card.title}</div>
                              <div className="mt-1 line-clamp-2 text-[10px] leading-4 text-zinc-500 dark:text-zinc-400">{card.description}</div>
                              <div className="mt-2 flex flex-wrap gap-1.5 text-[10px] text-zinc-500 dark:text-zinc-400">
                                <span className="rounded-full bg-zinc-100 px-2 py-0.5 dark:bg-zinc-800">
                                  Attempt {card.attempts}
                                </span>
                                {card.review_findings.length > 0 && (
                                  <span className="rounded-full bg-amber-100 px-2 py-0.5 text-amber-700 dark:bg-amber-500/10 dark:text-amber-300">
                                    Findings {card.review_findings.length}
                                  </span>
                                )}
                                {card.files_touched.length > 0 && (
                                  <span className="rounded-full bg-zinc-100 px-2 py-0.5 dark:bg-zinc-800">
                                    Files {card.files_touched.length}
                                  </span>
                                )}
                              </div>
                            </button>
                          ))
                        ) : (
                          <div className="rounded-xl border border-zinc-200/80 bg-white/70 px-3 py-4 text-center text-[11px] text-zinc-500 dark:border-zinc-800 dark:bg-zinc-950/50 dark:text-zinc-400">
                            No subtasks match the current filter.
                          </div>
                        )}
                      </div>
                    </div>

                    <div className="glass-panel p-3.5">
                      {focusedSubtask ? (
                        <div className="space-y-4">
                          <div className="flex items-start justify-between gap-3">
                            <div>
                              <div className="mb-2 flex items-center gap-2">
                                <span className="rounded-md bg-zinc-100 px-1.5 py-0.5 font-mono text-[10px] text-zinc-700 dark:bg-zinc-800 dark:text-zinc-200">
                                  {focusedSubtask.id}
                                </span>
                                <span className="rounded-full border border-zinc-200 px-2 py-0.5 text-[10px] text-zinc-500 dark:border-zinc-700 dark:text-zinc-400">
                                  {getKindLabel(focusedSubtask.kind)}
                                </span>
                              </div>
                              <h3 className="text-[15px] font-semibold leading-6 text-zinc-900 dark:text-zinc-100">{focusedSubtask.title}</h3>
                              <p className="mt-2 text-[12px] leading-6 text-zinc-600 dark:text-zinc-300">{focusedSubtask.description}</p>
                            </div>
                            <span className={`shrink-0 rounded-full border px-2.5 py-1 text-[10px] font-medium ${getStatusColor(focusedSubtask.status)}`}>
                              {getStatusLabel(focusedSubtask.status)}
                            </span>
                          </div>

                          <div className="flex flex-wrap gap-1.5 text-[10px] text-zinc-500 dark:text-zinc-400">
                            <span className="rounded-full bg-zinc-100 px-2 py-1 dark:bg-zinc-800">Attempts {focusedSubtask.attempts}</span>
                            {focusedSubtask.files_touched.length > 0 && (
                              <span className="rounded-full bg-zinc-100 px-2 py-1 dark:bg-zinc-800">
                                Files {focusedSubtask.files_touched.length}
                              </span>
                            )}
                          {focusedSubtask.review_findings.length > 0 && (
                            <span className="rounded-full bg-amber-100 px-2 py-1 text-amber-700 dark:bg-amber-500/10 dark:text-amber-300">
                              Findings {focusedSubtask.review_findings.length}
                            </span>
                          )}
                          {focusedSubtask.isolated_workspace && (
                            <span className="rounded-full bg-sky-100 px-2 py-1 text-sky-700 dark:bg-sky-500/10 dark:text-sky-300">
                              Isolated Run
                            </span>
                          )}
                          <button
                            onClick={() => setSelectedSubtaskId(null)}
                            className="rounded-full border border-zinc-200 bg-white/70 px-2 py-1 text-[10px] font-medium text-zinc-500 transition-colors hover:text-zinc-700 dark:border-zinc-700 dark:bg-zinc-900/60 dark:text-zinc-400 dark:hover:text-zinc-200"
                          >
                              Follow Live Lane
                            </button>
                          </div>

                          {focusedSubtask.latest_implementation && (
                            <div>
                              <div className="mb-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-zinc-500 dark:text-zinc-400">
                                Latest Implementation
                              </div>
                              <div className="rounded-xl border border-zinc-200/80 bg-white/70 px-3 py-2.5 text-[11px] leading-5 text-zinc-700 dark:border-zinc-800 dark:bg-zinc-950/50 dark:text-zinc-300">
                                {focusedSubtask.latest_implementation}
                              </div>
                            </div>
                          )}

                          {focusedSubtask.latest_review && (
                            <div>
                              <div className="mb-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-zinc-500 dark:text-zinc-400">
                                Latest Review
                              </div>
                              <div className="rounded-xl border border-zinc-200/80 bg-white/70 px-3 py-2.5 text-[11px] leading-5 text-zinc-700 dark:border-zinc-800 dark:bg-zinc-950/50 dark:text-zinc-300">
                                {focusedSubtask.latest_review}
                              </div>
                            </div>
                          )}

                          {focusedSubtask.merge_conflict && (
                            <div>
                              <div className="mb-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-zinc-500 dark:text-zinc-400">
                                Merge Conflict
                              </div>
                              <div className="rounded-xl border border-rose-200/80 bg-rose-50/80 px-3 py-2.5 text-[11px] leading-5 text-rose-700 dark:border-rose-500/20 dark:bg-rose-500/10 dark:text-rose-300">
                                {focusedSubtask.merge_conflict}
                              </div>
                            </div>
                          )}

                          {focusedSubtask.review_findings.length > 0 && (
                            <div>
                              <div className="mb-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-zinc-500 dark:text-zinc-400">
                                Findings
                              </div>
                              <ul className="space-y-2 text-[11px] leading-5 text-zinc-700 dark:text-zinc-300">
                                {focusedSubtask.review_findings.map((finding, idx) => (
                                  <li key={idx} className="rounded-xl bg-zinc-100/80 px-3 py-2.5 dark:bg-zinc-900/80">
                                    {finding}
                                  </li>
                                ))}
                              </ul>
                            </div>
                          )}

                          {focusedSubtask.files_touched.length > 0 && (
                            <div>
                              <div className="mb-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-zinc-500 dark:text-zinc-400">
                                Touched Files
                              </div>
                              <div className="flex flex-wrap gap-1.5">
                                {focusedSubtask.files_touched.map(file => (
                                  <span key={file} className="rounded-md bg-zinc-100 px-2 py-1 font-mono text-[10px] text-zinc-700 dark:bg-zinc-800 dark:text-zinc-200">
                                    {file}
                                  </span>
                                ))}
                              </div>
                            </div>
                          )}

                          {focusedSubtask.isolated_workspace && (
                            <div>
                              <div className="mb-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-zinc-500 dark:text-zinc-400">
                                Isolated Workspace
                              </div>
                              <div className="rounded-xl border border-zinc-200/80 bg-white/70 px-3 py-2.5 font-mono text-[11px] leading-5 text-zinc-700 dark:border-zinc-800 dark:bg-zinc-950/50 dark:text-zinc-300">
                                {focusedSubtask.isolated_workspace}
                              </div>
                            </div>
                          )}
                        </div>
                      ) : (
                        <div className="flex h-full flex-col items-center justify-center text-center">
                          <p className="text-[12px] font-medium text-zinc-700 dark:text-zinc-300">No visible subtask selected</p>
                          <p className="mt-1 max-w-sm text-[11px] leading-5 text-zinc-500 dark:text-zinc-400">
                            Adjust the status filter or pick a subtask from the list to inspect its implementation summary and review findings.
                          </p>
                        </div>
                      )}
                    </div>
                  </div>
                </>
              ) : (
                <div className="glass-panel p-4">
                  <p className="text-[12px] font-medium text-zinc-700 dark:text-zinc-300">
                    Structured execution data is not available yet.
                  </p>
                  <p className="mt-1 text-[11px] leading-5 text-zinc-500 dark:text-zinc-400">
                    Switch to Document view to inspect the raw execution board, or rerun the code flow to regenerate `BLACKBOARD.json`.
                  </p>
                </div>
              )
            ) : (
              <div className="glass-panel overflow-hidden p-3.5">
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
          className="flex shrink-0 flex-col border-t border-zinc-200/50 bg-white/30 backdrop-blur-md dark:border-zinc-800/50 dark:bg-zinc-950/30"
          style={{ height: activityCollapsed ? 'auto' : `${activityHeight}px` }}
        >
          {!activityCollapsed && (
            <div
              onMouseDown={handleActivityResizeStart}
              className="flex h-3 cursor-row-resize items-center justify-center border-b border-zinc-200/30 bg-white/10 dark:border-zinc-800/30 dark:bg-zinc-900/10"
              title="Drag to resize"
            >
              <div className="h-1 w-10 rounded-full bg-zinc-300/80 dark:bg-zinc-700/80" />
            </div>
          )}
          <div className="flex shrink-0 items-center justify-between gap-3 border-b border-zinc-200/50 bg-zinc-100/30 px-3 py-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-zinc-600 backdrop-blur-sm dark:border-zinc-800/50 dark:bg-zinc-900/30 dark:text-zinc-400">
            <div className="flex items-center gap-2">
              <VscChevronRight className="h-3.5 w-3.5" />
              <span>Recent Activity</span>
              <span className="rounded-full bg-zinc-200/80 px-1.5 py-0.5 text-[10px] dark:bg-zinc-800/80">
                {Math.min(filteredEvents.length, 10)}
              </span>
            </div>
            <button
              onClick={() => setActivityCollapsed(prev => !prev)}
              className="rounded-md p-1 text-zinc-500 transition-colors hover:bg-zinc-200/60 hover:text-zinc-700 dark:text-zinc-400 dark:hover:bg-zinc-800/60 dark:hover:text-zinc-200"
              title={activityCollapsed ? 'Expand activity' : 'Collapse activity'}
            >
              {activityCollapsed ? <VscChevronUp className="h-4 w-4" /> : <VscChevronDown className="h-4 w-4" />}
            </button>
          </div>
          {!activityCollapsed && (
            <div className="custom-scrollbar flex flex-1 flex-col gap-2.5 overflow-y-auto p-3">
              {[...filteredEvents].reverse().slice(0, 10).map((ev, i) => (
                  <button
                    key={i}
                    onClick={() => {
                      if (ev.subtask_id) {
                        focusSubtask(ev.subtask_id);
                      }
                    }}
                  className="relative flex gap-2.5 text-left"
                >
                  {i < Math.min(filteredEvents.length, 10) - 1 && (
                    <div className="absolute bottom-[-12px] left-[7px] top-4 w-[1px] bg-zinc-200 dark:bg-zinc-800" />
                  )}

                  <div className="z-10 mt-1 flex h-[15px] w-[15px] shrink-0 items-center justify-center rounded-full border border-zinc-300 bg-white dark:border-zinc-700 dark:bg-zinc-900">
                    <div
                      className={`h-1.5 w-1.5 rounded-full ${
                        ev.status === 'completed' || ev.status === 'done'
                          ? 'bg-emerald-500'
                          : ev.status === 'failed'
                            ? 'bg-rose-500'
                            : ev.status === 'needs_fix'
                              ? 'bg-amber-500'
                              : 'bg-blue-500'
                      }`}
                    />
                  </div>

                  <div className="flex-1 pb-1">
                    <div className="flex items-baseline gap-2">
                      {ev.subtask_id && (
                        <span className="rounded bg-zinc-200 px-1 py-0.5 font-mono text-[10px] text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300">
                          {ev.subtask_id}
                        </span>
                      )}
                      <span className="text-[11px] font-medium leading-5 text-zinc-700 dark:text-zinc-300">
                        {ev.summary}
                      </span>
                    </div>
                  </div>
                </button>
              ))}
            </div>
          )}
          {activityCollapsed && (
            <div className="px-3 py-2 text-[11px] text-zinc-500 dark:text-zinc-400">
              Activity timeline is collapsed.
            </div>
          )}
        </div>
      )}
    </div>
  );
}
