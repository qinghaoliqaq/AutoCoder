import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { VscRefresh, VscChevronRight, VscLayoutSidebarRightOff } from 'react-icons/vsc';
import { BlackboardEvent } from '../types';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

interface BlackboardPanelProps {
  workspacePath: string | null;
  events: BlackboardEvent[];
  onClose: () => void;
}

export default function BlackboardPanel({ workspacePath, events, onClose }: BlackboardPanelProps) {
  const [activeBoard, setActiveBoard] = useState<'plan' | 'exec'>('plan');
  const [planMd, setPlanMd] = useState<string | null>(null);
  const [execMd, setExecMd] = useState<string | null>(null);
  const [execJson, setExecJson] = useState<any | null>(null);
  const [loading, setLoading] = useState(false);
  const autoSwitchedExecRef = useRef(false);

  useEffect(() => {
    autoSwitchedExecRef.current = false;
    setActiveBoard('plan');
  }, [workspacePath]);
  
  const loadBoards = useCallback(async () => {
    if (!workspacePath) {
      setPlanMd(null);
      setExecMd(null);
      setExecJson(null);
      return;
    }
    
    setLoading(true);
    
    try {
      // Try to load PLAN_BLACKBOARD.md or PLAN.md
      try {
        const pMd = await invoke<string>('read_workspace_file', { 
          path: workspacePath, 
          relativePath: 'PLAN_BLACKBOARD.md' 
        });
        setPlanMd(pMd);
      } catch (e) {
        // Fallback to PLAN.md which is often used in the plan phase
        try {
          const pMd = await invoke<string>('read_workspace_file', { 
            path: workspacePath, 
            relativePath: 'PLAN.md' 
          });
          setPlanMd(pMd);
        } catch (e2) {
          setPlanMd(null);
        }
      }
      
      // Try to load BLACKBOARD.md
      try {
        const eMd = await invoke<string>('read_workspace_file', { 
          path: workspacePath, 
          relativePath: 'BLACKBOARD.md' 
        });
        setExecMd(eMd);
      } catch (e) {
        setExecMd(null);
      }
      
      // Try to load BLACKBOARD.json for status info
      try {
        const eJsonStr = await invoke<string>('read_workspace_file', { 
          path: workspacePath, 
          relativePath: 'BLACKBOARD.json' 
        });
        setExecJson(JSON.parse(eJsonStr));
        // Auto-switch only once when exec board first appears for this workspace.
        if (!autoSwitchedExecRef.current) {
          setActiveBoard('exec');
          autoSwitchedExecRef.current = true;
        }
      } catch (e) {
        setExecJson(null);
      }
    } catch (err) {
      console.error('Error loading blackboards:', err);
    } finally {
      setLoading(false);
    }
  }, [workspacePath]);
  
  // Refresh when workspace changes or new events arrive
  useEffect(() => {
    loadBoards();
  }, [loadBoards, events.length]);
  
  // Custom markdown renderer to make it look less generic
  const renderMarkdown = (content: string) => {
    return (
      <div className="prose prose-sm dark:prose-invert max-w-none prose-h1:text-lg prose-h2:text-base prose-h3:text-sm prose-p:leading-snug prose-li:leading-snug prose-a:text-violet-500 hover:prose-a:text-violet-600">
        <ReactMarkdown remarkPlugins={[remarkGfm]}>
          {content}
        </ReactMarkdown>
      </div>
    );
  };
  
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
  
  return (
    <div className="flex flex-col h-full bg-transparent font-sans">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-zinc-200/50 dark:border-zinc-800/50 glass-header shrink-0">
        <div>
          <h2 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200 flex items-center gap-2">
            <span className="text-violet-500">📋</span> BLACKBOARD
          </h2>
          <p className="text-xs text-zinc-500 mt-0.5 truncate max-w-[180px]" title={workspacePath || 'No workspace'}>
            {workspacePath ? workspacePath.split('/').pop() : 'No active workspace'}
          </p>
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={loadBoards}
            className={`p-1.5 text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300 rounded-md hover:bg-zinc-200/50 dark:hover:bg-zinc-800/50 transition-colors ${loading ? 'animate-spin' : ''}`}
            title="Refresh Boards"
          >
            <VscRefresh className="w-4 h-4" />
          </button>
          <button
            onClick={onClose}
            className="p-1.5 text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300 rounded-md hover:bg-zinc-200/50 dark:hover:bg-zinc-800/50 transition-colors"
            title="Close Panel"
          >
            <VscLayoutSidebarRightOff className="w-4 h-4" />
          </button>
        </div>
      </div>
      
      {/* Active subtask / Status Summary */}
      {workspacePath && (
        <div className="px-4 py-3 border-b border-zinc-200/50 dark:border-zinc-800/50 bg-white/40 dark:bg-zinc-900/30 backdrop-blur-md shrink-0">
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-medium text-zinc-500 dark:text-zinc-400 uppercase tracking-wider">Mission Status</span>
            {execJson?.state && (
              <span className={`text-[10px] px-2 py-0.5 rounded-full border ${getStatusColor(execJson.state)}`}>
                {execJson.state.replace('_', ' ').toUpperCase()}
              </span>
            )}
          </div>
          
          {execJson?.active_subtask_id ? (
            <div className="bg-blue-50/50 dark:bg-blue-900/10 border border-blue-100 dark:border-blue-900/30 rounded-lg p-3">
              <div className="flex items-center gap-2 mb-1">
                <span className="w-2 h-2 rounded-full bg-blue-500 animate-pulse" />
                <span className="text-xs font-semibold text-blue-700 dark:text-blue-400">
                  Executing: {execJson.active_subtask_id}
                </span>
              </div>
              <p className="text-xs text-zinc-600 dark:text-zinc-300 line-clamp-2">
                {execJson.subtasks?.find((s: any) => s.id === execJson.active_subtask_id)?.title || 'Working on task...'}
              </p>
            </div>
          ) : (
            <div className="bg-zinc-100/50 dark:bg-zinc-800/50 border border-zinc-200 dark:border-zinc-700/50 rounded-lg p-3">
              <span className="text-xs text-zinc-500 dark:text-zinc-400">
                {planMd ? 'Plan active. Waiting for code execution to start.' : 'No active tasks.'}
              </span>
            </div>
          )}
        </div>
      )}
      
      {/* Board Switcher */}
      {workspacePath && (
        <div className="flex p-2 gap-1 bg-white/20 dark:bg-zinc-900/40 backdrop-blur-sm shrink-0 border-b border-zinc-200/50 dark:border-zinc-800/50">
          <button
            onClick={() => setActiveBoard('plan')}
            className={`flex-1 py-1.5 px-3 text-xs font-medium rounded-md transition-all ${
              activeBoard === 'plan' 
                ? 'bg-white/80 dark:bg-zinc-800/80 text-zinc-800 dark:text-zinc-200 shadow-sm border border-white/60 dark:border-zinc-700/50 backdrop-blur-md' 
                : 'text-zinc-500 hover:text-zinc-700 dark:hover:text-zinc-300 hover:bg-white/40 dark:hover:bg-zinc-800/40 border border-transparent'
            }`}
          >
            Plan Board
            {planMd && <span className="ml-1.5 w-1.5 h-1.5 inline-block rounded-full bg-emerald-400" />}
          </button>
          <button
            onClick={() => setActiveBoard('exec')}
            className={`flex-1 py-1.5 px-3 text-xs font-medium rounded-md transition-all ${
              activeBoard === 'exec' 
                ? 'bg-white/80 dark:bg-zinc-800/80 text-zinc-800 dark:text-zinc-200 shadow-sm border border-white/60 dark:border-zinc-700/50 backdrop-blur-md' 
                : 'text-zinc-500 hover:text-zinc-700 dark:hover:text-zinc-300 hover:bg-white/40 dark:hover:bg-zinc-800/40 border border-transparent'
            }`}
          >
            Exec Board
            {execMd && <span className="ml-1.5 w-1.5 h-1.5 inline-block rounded-full bg-blue-400" />}
          </button>
        </div>
      )}
      
      {/* Main Content Area */}
      <div className="flex-1 overflow-y-auto p-4 custom-scrollbar">
        {!workspacePath ? (
          <div className="h-full flex flex-col items-center justify-center text-center px-4">
            <div className="w-12 h-12 rounded-full bg-zinc-100 dark:bg-zinc-800 flex items-center justify-center mb-3">
              <span className="text-xl">📋</span>
            </div>
            <p className="text-sm font-medium text-zinc-700 dark:text-zinc-300 mb-1">No Workspace Active</p>
            <p className="text-xs text-zinc-500 dark:text-zinc-400">
              Open a project to view its planning and execution blackboards.
            </p>
          </div>
        ) : activeBoard === 'plan' ? (
          planMd ? (
            <div className="glass-panel p-4 overflow-hidden">
              {renderMarkdown(planMd)}
            </div>
          ) : (
            <div className="text-center py-8">
              <p className="text-xs text-zinc-500">No planning board found (PLAN.md)</p>
            </div>
          )
        ) : (
          execMd ? (
            <div className="glass-panel p-4 overflow-hidden">
              {renderMarkdown(execMd)}
            </div>
          ) : (
            <div className="text-center py-8">
              <p className="text-xs text-zinc-500">No execution board found (BLACKBOARD.md)</p>
              <p className="text-[10px] text-zinc-400 mt-2">Run the 'code' skill to generate it.</p>
            </div>
          )
        )}
      </div>
      
      {/* Recent Events Timeline */}
      {events.length > 0 && (
        <div className="h-48 border-t border-zinc-200/50 dark:border-zinc-800/50 bg-white/30 dark:bg-zinc-950/30 backdrop-blur-md flex flex-col shrink-0">
          <div className="px-3 py-2 border-b border-zinc-200/50 dark:border-zinc-800/50 bg-zinc-100/30 dark:bg-zinc-900/30 text-xs font-semibold text-zinc-600 dark:text-zinc-400 flex items-center gap-2 shrink-0 backdrop-blur-sm">
            <VscChevronRight className="w-3.5 h-3.5" />
            Recent Activity
          </div>
          <div className="flex-1 overflow-y-auto p-3 custom-scrollbar flex flex-col gap-3">
            {[...events].reverse().slice(0, 10).map((ev, i) => (
              <div key={i} className="flex gap-2.5 relative">
                {/* Timeline line */}
                {i < Math.min(events.length, 10) - 1 && (
                  <div className="absolute left-[7px] top-4 bottom-[-12px] w-[1px] bg-zinc-200 dark:bg-zinc-800" />
                )}
                
                {/* Dot */}
                <div className="mt-1 w-[15px] h-[15px] rounded-full flex items-center justify-center bg-white dark:bg-zinc-900 border border-zinc-300 dark:border-zinc-700 z-10 shrink-0">
                  <div className={`w-1.5 h-1.5 rounded-full ${
                    ev.status === 'completed' || ev.status === 'done' ? 'bg-emerald-500' :
                    ev.status === 'failed' ? 'bg-rose-500' :
                    ev.status === 'needs_fix' ? 'bg-amber-500' :
                    'bg-blue-500'
                  }`} />
                </div>
                
                {/* Content */}
                <div className="flex-1 pb-1">
                  <div className="flex items-baseline gap-2">
                    {ev.subtask_id && (
                      <span className="text-[10px] font-mono px-1 py-0.5 rounded bg-zinc-200 dark:bg-zinc-800 text-zinc-600 dark:text-zinc-300">
                        {ev.subtask_id}
                      </span>
                    )}
                    <span className="text-xs text-zinc-700 dark:text-zinc-300 font-medium">
                      {ev.summary}
                    </span>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
