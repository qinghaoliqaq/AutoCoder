import React, { useState, useEffect, useCallback, useRef } from 'react';
import ReactDOM from 'react-dom/client';
import { invoke } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { ErrorBoundary } from './components/ErrorBoundary';
import { Toaster } from 'sonner';
import './index.css';

import { AppMode, ChatMessage, ToolLog, TokenUsage, MODES, SessionMeta, BlackboardEvent } from './types';
import { ThemeProvider, useTheme, THEMES } from './components/ThemeProvider';
import ModeActivated from './components/ModeActivated';
import ChatPanel from './components/ChatPanel';
import InputBar from './components/InputBar';
import FileTreePanel from './components/FileTreePanel';
import ToolLogPanel from './components/ToolLogPanel';
import HistoryPanel from './components/HistoryPanel';
import BlackboardPanel from './components/BlackboardPanel';
import ConfigEditorModal from './components/ConfigEditorModal';
import ProjectContextEditorModal from './components/ProjectContextEditorModal';
import { VscColorMode, VscFiles, VscHistory, VscMultipleWindows, VscTerminal, VscChecklist, VscSettingsGear } from 'react-icons/vsc';
import { makeSessionId, syncSessionIdentity } from './utils';
import { useSessionManager } from './hooks/useSessionManager';
import { useDirectorLoop } from './hooks/useDirectorLoop';
import { useConfigState } from './hooks/useConfigState';
import { useSidebarState } from './hooks/useSidebarState';

// ── Small helpers ────────────────────────────────────────────────────────────

function ThemeToggle() {
  const { activeTheme, isDark, themePreference, setTheme } = useTheme();
  const cycle = () => {
    const ids = ['system', ...THEMES.map(t => t.id)];
    const idx = ids.indexOf(themePreference);
    setTheme(ids[(idx + 1) % ids.length]);
  };
  return (
    <button
      onClick={cycle}
      className="text-xs text-content-secondary hover:text-content-primary transition-colors flex items-center gap-1.5 px-3 py-1.5 rounded-lg w-full h-full"
      title={`Theme: ${themePreference === 'system' ? 'System' : activeTheme.label}`}
    >
      {themePreference === 'system'
        ? <VscColorMode className="w-3.5 h-3.5 animate-[spin_4s_linear_infinite]" />
        : isDark ? '☾' : '☀'}
      <span className="hidden sm:inline font-medium">
        {themePreference === 'system' ? 'System' : activeTheme.label}
      </span>
    </button>
  );
}

// ── Main component ───────────────────────────────────────────────────────────

function DevHub() {
  // ── Core state ──────────────────────────────────────────────────────────
  const [currentMode, setCurrentMode] = useState<AppMode | null>(null);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [workspace, setWorkspace] = useState<string | null>(null);
  const [projectContext, setProjectContext] = useState<string | null>(null);
  const [showContextEditor, setShowContextEditor] = useState(false);
  const [contextDraft, setContextDraft] = useState('');
  const [toolLogs, setToolLogs] = useState<ToolLog[]>([]);
  const [tokenUsages, setTokenUsages] = useState<TokenUsage[]>([]);
  const [blackboardEvents, setBlackboardEvents] = useState<BlackboardEvent[]>([]);
  const [currentSessionId, setCurrentSessionId] = useState<string>(makeSessionId);
  const [sessions, setSessions] = useState<SessionMeta[]>([]);

  // ── Refs (sync wrappers for closures) ──────────────────────────────────
  const planReportRef = useRef<string>('');
  const projectContextRef = useRef<string | null>(projectContext);
  projectContextRef.current = projectContext;
  const projectContextMetaRef = useRef<{ source: 'auto' | 'manual' | null; workspace: string | null }>({
    source: null, workspace: null,
  });
  const sessionIdRef = useRef(currentSessionId);
  sessionIdRef.current = currentSessionId;
  const stopRequestedRef = useRef(false);
  const workspaceRef = useRef(workspace);
  workspaceRef.current = workspace;
  const messagesRef = useRef(messages);
  messagesRef.current = messages;
  const toolLogsRef = useRef(toolLogs);
  toolLogsRef.current = toolLogs;
  const blackboardEventsRef = useRef(blackboardEvents);
  blackboardEventsRef.current = blackboardEvents;

  // ── Composed hooks ─────────────────────────────────────────────────────
  const config = useConfigState();
  const sidebar = useSidebarState(messages);

  const { handleSubmit, isRunning, isStopping, handleStop } = useDirectorLoop({
    workspaceRef, projectContextRef, projectContextMetaRef, planReportRef, stopRequestedRef,
    setMessages, setCurrentMode, setToolLogs, setTokenUsages, setBlackboardEvents, setWorkspace,
  });

  // ── Session history ────────────────────────────────────────────────────
  useEffect(() => {
    invoke<SessionMeta[]>('list_sessions', { workspace }).then(setSessions).catch(() => {});
  }, [workspace]);

  const { flushPendingSessionSave, handleLoadSession: rawLoadSession, handleDeleteSession: rawDeleteSession } = useSessionManager({
    workspace, currentSessionId, messages, sessions,
    messagesRef, toolLogsRef, blackboardEventsRef, projectContextRef,
    projectContextMetaRef, sessionIdRef, planReportRef,
    setMessages, setToolLogs, setBlackboardEvents, setCurrentSessionId,
    setSessions, setWorkspace, setProjectContext,
  });

  const handleLoadSession = useCallback(async (sessionId: string) => {
    setTokenUsages([]);
    await rawLoadSession(sessionId);
  }, [rawLoadSession]);

  const handleDeleteSession = useCallback(async (sessionId: string) => {
    await rawDeleteSession(sessionId);
    if (sessionId === currentSessionId) setTokenUsages([]);
  }, [rawDeleteSession, currentSessionId]);

  // ── Workspace side-effects ─────────────────────────────────────────────

  // Auto-show tool logs the first time
  const hasAutoShownLogsRef = useRef(false);
  useEffect(() => {
    if (toolLogs.length > 0 && !hasAutoShownLogsRef.current && sidebar.activeSidebarTab === null) {
      hasAutoShownLogsRef.current = true;
      sidebar.setActiveSidebarTab('logs');
    }
  }, [toolLogs.length, sidebar.activeSidebarTab]);

  // Auto-show explorer on workspace change
  const lastExplorerWorkspaceRef = useRef<string | null>(null);
  useEffect(() => {
    if (workspace && workspace !== lastExplorerWorkspaceRef.current) {
      lastExplorerWorkspaceRef.current = workspace;
      sidebar.setActiveSidebarTab('explorer');
    }
  }, [workspace]);

  // Keep context bound to workspace
  useEffect(() => {
    if (!workspace || projectContextRef.current === null) return;
    const meta = projectContextMetaRef.current;
    if (meta.source === 'manual' && meta.workspace === null) {
      projectContextMetaRef.current = { ...meta, workspace };
      return;
    }
    if (meta.workspace && meta.workspace !== workspace) {
      projectContextMetaRef.current = { source: null, workspace: null };
      projectContextRef.current = null;
      setProjectContext(null);
    }
  }, [workspace]);

  // Auto-load project docs
  useEffect(() => {
    if (!workspace || projectContext !== null) return;
    let cancelled = false;
    const startedForWorkspace = workspace;
    invoke<{ content: string; filenames: string[] }>('read_project_docs', { path: workspace })
      .then(docs => {
        if (cancelled) return;
        if (docs.filenames.length > 0) {
          projectContextMetaRef.current = { source: 'auto', workspace: startedForWorkspace };
          projectContextRef.current = docs.content;
          setProjectContext(docs.content);
        }
      })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [workspace, projectContext]);

  // Sanitize blackboard on workspace open
  useEffect(() => {
    if (workspace) {
      invoke('sanitize_blackboard_state', { path: workspace }).catch(() => {});
    }
  }, [workspace]);

  // Keep PLAN.md in sync
  useEffect(() => {
    let cancelled = false;
    if (!workspace) { planReportRef.current = ''; return; }
    invoke<string>('read_workspace_file', {
      path: workspace, relativePath: '.ai-dev-hub/PLAN.md',
    })
      .then(plan => { if (!cancelled) planReportRef.current = plan; })
      .catch(() => { if (!cancelled) planReportRef.current = ''; });
    return () => { cancelled = true; };
  }, [workspace]);

  // ── Workspace / session callbacks ──────────────────────────────────────

  const handleOpenProject = useCallback(async () => {
    const selected = await openDialog({ directory: true, multiple: false, title: '选择项目文件夹' });
    if (!selected) return;
    try {
      const validated = await invoke<string>('open_project', { path: selected as string });
      const switchingWorkspace = workspaceRef.current !== validated;
      if (switchingWorkspace) {
        await flushPendingSessionSave();
        projectContextMetaRef.current = { source: null, workspace: null };
        projectContextRef.current = null;
        setProjectContext(null);
        setMessages([]);
        setToolLogs([]);
        setBlackboardEvents([]);
        planReportRef.current = '';
        hasAutoShownLogsRef.current = false;
        syncSessionIdentity(makeSessionId(), sessionIdRef, setCurrentSessionId);
        await invoke('clear_history');
      } else {
        const meta = projectContextMetaRef.current;
        if (meta.workspace && meta.workspace !== validated) {
          projectContextMetaRef.current = { source: null, workspace: null };
          projectContextRef.current = null;
          setProjectContext(null);
        }
      }
      setWorkspace(validated);
    } catch (err) {
      console.error('open_project error:', err);
    }
  }, [flushPendingSessionSave]);

  const handleNewChat = useCallback(async () => {
    await flushPendingSessionSave();
    setMessages([]);
    setToolLogs([]);
    setTokenUsages([]);
    setBlackboardEvents([]);
    planReportRef.current = '';
    hasAutoShownLogsRef.current = false;
    syncSessionIdentity(makeSessionId(), sessionIdRef, setCurrentSessionId);
    await invoke('clear_history').catch(console.error);
  }, [flushPendingSessionSave]);

  const handleNewWindow = useCallback(() => {
    invoke('open_new_window').catch(console.error);
  }, []);

  // ── Context editor helpers ─────────────────────────────────────────────

  const handleSaveContext = useCallback((draft: string) => {
    const next = draft.trim() || null;
    projectContextMetaRef.current = next
      ? { source: 'manual', workspace: workspaceRef.current }
      : { source: null, workspace: null };
    projectContextRef.current = next;
    setProjectContext(next);
    setShowContextEditor(false);
  }, []);

  const handleClearContext = useCallback(() => {
    projectContextMetaRef.current = { source: null, workspace: null };
    projectContextRef.current = null;
    setProjectContext(null);
    setContextDraft('');
    setShowContextEditor(false);
  }, []);

  // ── Render ─────────────────────────────────────────────────────────────

  const modeLabel = currentMode
    ? (MODES.find((m) => m.id === currentMode)?.label ?? currentMode)
    : null;

  return (
    <ThemeProvider>
      <ModeActivated mode={currentMode} />

      {/* Background blobs */}
      <div className="fixed inset-0 z-0 bg-surface-primary pointer-events-none overflow-hidden">
        <div className="absolute top-[-10%] left-[-10%] w-[40%] h-[40%] rounded-full blur-[100px] animate-blob" style={{ backgroundColor: 'rgb(var(--accent) / 0.12)' }} />
        <div className="absolute bottom-[-10%] right-[-10%] w-[40%] h-[40%] rounded-full blur-[100px] animate-blob" style={{ backgroundColor: 'rgb(var(--accent) / 0.08)', animationDelay: '2s' }} />
        <div className="absolute top-[20%] right-[20%] w-[30%] h-[30%] rounded-full blur-[100px] animate-blob" style={{ backgroundColor: 'rgb(var(--accent) / 0.06)', animationDelay: '4s' }} />
      </div>

      <div className="flex flex-col h-screen w-screen overflow-hidden bg-transparent font-sans animate-app-entrance relative z-10 text-content-primary">

        {/* Header */}
        <header className="flex items-center justify-between pr-5 pl-24 h-12 flex-shrink-0 glass-header relative z-50 select-none">
          <div className="flex items-center gap-2.5 relative z-50 pointer-events-auto shrink-0">
            {modeLabel && (
              <div className="flex items-center gap-1.5 rounded-lg border border-violet-200/50 bg-violet-50/50 px-2.5 py-1 dark:border-violet-500/20 dark:bg-violet-500/10">
                <span className="h-1.5 w-1.5 rounded-full bg-violet-500 animate-pulse" />
                <span className="text-[11px] font-semibold text-violet-600 dark:text-violet-400">{modeLabel}</span>
              </div>
            )}
            {isRunning && !modeLabel && (
              <div className="flex items-center gap-1.5 rounded-lg border border-blue-200/50 bg-blue-50/50 px-2.5 py-1 dark:border-blue-500/20 dark:bg-blue-500/10">
                <span className="h-1.5 w-1.5 rounded-full bg-blue-500 animate-pulse" />
                <span className="text-[11px] font-semibold text-blue-600 dark:text-blue-400">Running</span>
              </div>
            )}
          </div>

          <div data-tauri-drag-region className="flex-1 h-full self-stretch drag-region" style={{ WebkitAppRegion: 'drag' } as React.CSSProperties} />

          <div className="flex items-center gap-1.5 relative z-50 pointer-events-auto shrink-0">
            <button onClick={config.handleOpenConfigEditor} className="flex h-8 items-center gap-1.5 rounded-lg px-2.5 text-[11px] text-content-secondary transition-all hover:bg-surface-tertiary/50 hover:text-content-primary" title="Settings">
              <VscSettingsGear className="h-3.5 w-3.5" />
              <span className="hidden sm:inline font-medium">Settings</span>
            </button>
            <button
              onClick={() => { setContextDraft(projectContext ?? ''); setShowContextEditor(true); }}
              className={`flex h-8 items-center gap-1.5 rounded-lg px-2.5 text-[11px] transition-all hover:bg-surface-tertiary/50 ${projectContext ? 'text-themed-accent-text' : 'text-content-secondary hover:text-content-primary'}`}
              title="Project Context"
            >
              <svg className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth={1.5} viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m2.25 0H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
              </svg>
              {projectContext && <span className="hidden sm:inline font-medium">{(projectContext.length / 1024).toFixed(1)}K</span>}
            </button>
            <button onClick={handleNewWindow} className="flex h-8 items-center gap-1.5 rounded-lg px-2.5 text-[11px] text-content-secondary transition-all hover:bg-surface-tertiary/50 hover:text-content-primary" title="New Window">
              <VscMultipleWindows className="w-3.5 h-3.5" />
            </button>
            <div className="h-4 w-px bg-edge-primary" />
            <div className="rounded-lg transition-all hover:bg-surface-tertiary/50"><ThemeToggle /></div>
          </div>
        </header>

        {/* Main area */}
        <div className="flex-1 flex min-h-0 bg-transparent overflow-hidden relative">

          {/* Activity Bar */}
          <div className="w-12 h-full flex flex-col items-center py-3 gap-1 glass-container border-r border-edge-primary/50 z-30 flex-shrink-0">
            {([
              { tab: 'explorer' as const, icon: <VscFiles className="w-[18px] h-[18px]" />, title: 'Explorer', badge: false },
              { tab: 'logs' as const, icon: <VscTerminal className="w-[18px] h-[18px]" />, title: 'Tool Logs', badge: toolLogs.length > 0 && sidebar.activeSidebarTab !== 'logs' },
              { tab: 'history' as const, icon: <VscHistory className="w-[18px] h-[18px]" />, title: 'History', badge: sessions.length > 0 && sidebar.activeSidebarTab !== 'history' },
            ] as const).map(({ tab, icon, title, badge }) => (
              <button
                key={tab}
                onClick={() => sidebar.toggleSidebarTab(tab)}
                className={`relative flex h-9 w-9 items-center justify-center rounded-xl transition-all duration-200 ${
                  sidebar.activeSidebarTab === tab
                    ? 'bg-surface-elevated text-themed-accent-text shadow-sm ring-1 ring-edge-primary/60'
                    : 'text-content-tertiary hover:bg-surface-tertiary/40 hover:text-content-primary'
                }`}
                title={title}
              >
                {icon}
                {badge && <span className="absolute -right-0.5 -top-0.5 h-2 w-2 rounded-full bg-blue-500 ring-[1.5px] ring-white shadow-[0_0_6px_rgba(59,130,246,0.5)] dark:ring-zinc-950" />}
              </button>
            ))}
            <div className="my-1 h-px w-5 bg-edge-primary/60" />
            <button
              onClick={sidebar.toggleBlackboardWorkspace}
              className={`relative flex h-9 w-9 items-center justify-center rounded-xl transition-all duration-200 ${
                sidebar.activeSidebarTab === 'blackboard'
                  ? 'bg-surface-elevated text-themed-accent-text shadow-sm ring-1 ring-edge-primary/60'
                  : 'text-content-tertiary hover:bg-surface-tertiary/40 hover:text-content-primary'
              }`}
              title="Blackboard"
            >
              <VscChecklist className="w-[18px] h-[18px]" />
              {(blackboardEvents.length > 0 || sidebar.unreadAgentMessages > 0) && sidebar.activeSidebarTab !== 'blackboard' && (
                <span className="absolute -right-0.5 -top-0.5 h-2 w-2 rounded-full bg-violet-500 ring-[1.5px] ring-white shadow-[0_0_6px_rgba(139,92,246,0.5)] dark:ring-zinc-950" />
              )}
            </button>
          </div>

          {/* Sidebar */}
          <div
            className={`h-full border-r border-edge-primary/50 glass-container flex-shrink-0 overflow-hidden transition-[width] duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] z-20 ${
              sidebar.activeSidebarTab !== null && !sidebar.blackboardFullscreen ? 'opacity-100' : 'opacity-0 border-none'
            }`}
            style={{ width: sidebar.sidebarWidth }}
          >
            <div className="h-full relative" style={{ width: sidebar.sidebarWidth }}>
              <div className={`absolute inset-0 transition-opacity duration-300 ${sidebar.activeSidebarTab === 'explorer' ? 'opacity-100 z-10 pointer-events-auto' : 'opacity-0 z-0 pointer-events-none'}`}>
                <FileTreePanel workspacePath={workspace} onOpenProject={handleOpenProject} onClose={() => sidebar.setActiveSidebarTab(null)} />
              </div>
              <div className={`absolute inset-0 transition-opacity duration-300 ${sidebar.activeSidebarTab === 'logs' ? 'opacity-100 z-10 pointer-events-auto' : 'opacity-0 z-0 pointer-events-none'}`}>
                <ToolLogPanel logs={toolLogs} tokenUsages={tokenUsages} onClose={() => sidebar.setActiveSidebarTab(null)} />
              </div>
              <div className={`absolute inset-0 transition-opacity duration-300 ${sidebar.activeSidebarTab === 'history' ? 'opacity-100 z-10 pointer-events-auto' : 'opacity-0 z-0 pointer-events-none'}`}>
                <HistoryPanel sessions={sessions} currentSessionId={currentSessionId} onLoad={handleLoadSession} onDelete={handleDeleteSession} onNewChat={handleNewChat} onClose={() => sidebar.setActiveSidebarTab(null)} />
              </div>
              <div className={`absolute inset-0 transition-opacity duration-300 ${sidebar.activeSidebarTab === 'blackboard' && !sidebar.blackboardFullscreen ? 'opacity-100 z-10 pointer-events-auto' : 'opacity-0 z-0 pointer-events-none'}`}>
                <BlackboardPanel workspacePath={workspace} events={blackboardEvents} onClose={() => sidebar.setActiveSidebarTab(null)} />
              </div>
            </div>
          </div>

          {/* Main content */}
          {sidebar.blackboardFullscreen ? (
            <div className="flex-1 min-w-0 z-10 basis-0 bg-transparent shadow-[-4px_0_15px_-5px_rgba(0,0,0,0.05)] dark:shadow-[-4px_0_15px_-5px_rgba(0,0,0,0.4)]">
              <BlackboardPanel workspacePath={workspace} events={blackboardEvents} onClose={sidebar.closeBlackboardWorkspace} onBack={sidebar.closeBlackboardWorkspace} fullscreen unreadAgentMessages={sidebar.unreadAgentMessages} />
            </div>
          ) : (
            <div className="flex flex-1 min-h-0 min-w-0 basis-0 flex-col overflow-hidden relative z-10 bg-transparent shadow-[-4px_0_15px_-5px_rgba(0,0,0,0.05)] dark:shadow-[-4px_0_15px_-5px_rgba(0,0,0,0.4)]">
              <ChatPanel messages={messages} toolLogs={toolLogs} onOpenProject={handleOpenProject} workspace={workspace} />
              <div className="absolute bottom-0 left-0 right-0 p-4 sm:p-6 pointer-events-none z-10 flex flex-col justify-end"
                   style={{ background: `linear-gradient(to top, rgb(var(--bg-primary)), rgb(var(--bg-primary) / 0.8), transparent)` }}>
                <div className="pointer-events-auto max-w-4xl w-full mx-auto">
                  <InputBar
                    mode={currentMode ?? 'chat'}
                    configStatus={config.configStatus}
                    isRunning={isRunning}
                    isStopping={isStopping}
                    configUpdating={config.configUpdating}
                    onSubmit={handleSubmit}
                    onStop={handleStop}
                    onToggleExecutionAccess={config.handleToggleExecutionAccess}
                  />
                </div>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Modals */}
      {config.showConfigEditor && (
        <ConfigEditorModal
          draft={config.configDraft}
          saving={config.configSaving}
          error={config.configError}
          onClose={config.closeConfigEditor}
          onChange={config.setConfigDraft}
          onSave={config.handleSaveConfig}
        />
      )}
      {showContextEditor && (
        <ProjectContextEditorModal
          draft={contextDraft}
          onChange={setContextDraft}
          onSave={handleSaveContext}
          onClear={handleClearContext}
          onClose={() => setShowContextEditor(false)}
        />
      )}
    </ThemeProvider>
  );
}

// ── Mount ────────────────────────────────────────────────────────────────────

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <DevHub />
      <Toaster
        position="bottom-right"
        theme="system"
        richColors
        closeButton
        toastOptions={{
          className: 'text-sm',
          duration: 5000,
        }}
      />
    </ErrorBoundary>
  </React.StrictMode>,
);
