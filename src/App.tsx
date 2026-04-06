import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { open as openDialog } from '@tauri-apps/plugin-dialog';

// Scope all event listeners to the current window so multiple windows
// don't receive each other's skill-chunk / tool-log / director events.
const appWindow = getCurrentWebviewWindow();
import { AppMode, ChatMessage, ToolLog, SystemStatus, ConfigStatus, ConfigDraft, MODES, SessionMeta, BlackboardEvent } from './types';
import { ThemeProvider, useTheme } from './components/ThemeProvider';
import ModeActivated from './components/ModeActivated';
import ChatPanel from './components/ChatPanel';
import InputBar from './components/InputBar';
import FileTreePanel from './components/FileTreePanel';
import ToolLogPanel from './components/ToolLogPanel';
import HistoryPanel from './components/HistoryPanel';
import BlackboardPanel from './components/BlackboardPanel';
import ConfigEditorModal from './components/ConfigEditorModal';
import { VscColorMode, VscFiles, VscHistory, VscMultipleWindows, VscTerminal, VscChecklist, VscSettingsGear } from 'react-icons/vsc';

function ThemeToggle() {
  const { theme, setTheme } = useTheme();
  return (
    <button
      onClick={() => setTheme(theme === 'dark' ? 'light' : theme === 'light' ? 'system' : 'dark')}
      className="text-xs text-zinc-600 hover:text-zinc-800 dark:text-zinc-300 dark:hover:text-zinc-100 transition-colors flex items-center gap-1.5 px-3 py-1.5 rounded-lg w-full h-full"
      title={`Current theme: ${theme}`}
    >
      {theme === 'dark' ? '☾' : theme === 'light' ? '☀' : <VscColorMode className="w-3.5 h-3.5 animate-[spin_4s_linear_infinite]" />}
      <span className="hidden sm:inline font-medium capitalize">{theme}</span>
    </button>
  );
}

// ── helpers ────────────────────────────────────────────────────────────────────


import { parseInvoke, stripInvoke } from './invoke';
import { buildNextInputAfterReview, buildNextInputAfterQaWithEvidence, buildNextInputAfterTestWithEvidence, buildNextInputAfterCodeWithEvidence } from './directorFlow';
import { makeId, makeSessionId, syncSessionIdentity } from './utils';
import { useSessionManager } from './hooks/useSessionManager';
import { createSkillRunner } from './hooks/useSkillRunner';

// ── App ────────────────────────────────────────────────────────────────────────

export default function App() {
  type SidebarTab = 'explorer' | 'logs' | 'history' | 'blackboard';
  const [currentMode, setCurrentMode] = useState<AppMode | null>(null);
  const [status, setStatus] = useState<SystemStatus | null>(null);
  const [configStatus, setConfigStatus] = useState<ConfigStatus | null>(null);
  const [checking, setChecking] = useState(true);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isRunning, setIsRunning] = useState(false);
  const [isStopping, setIsStopping] = useState(false);
  const [workspace, setWorkspace] = useState<string | null>(null);
  const [projectContext, setProjectContext] = useState<string | null>(null);
  const [showContextEditor, setShowContextEditor] = useState(false);
  const [contextDraft, setContextDraft] = useState('');
  const [showConfigEditor, setShowConfigEditor] = useState(false);
  const [configDraft, setConfigDraft] = useState<ConfigDraft | null>(null);
  const [configSaving, setConfigSaving] = useState(false);
  const [configError, setConfigError] = useState<string | null>(null);
  const [configUpdating, setConfigUpdating] = useState(false);
  const [activeSidebarTab, setActiveSidebarTab] = useState<SidebarTab | null>(null);
  const [blackboardFullscreen, setBlackboardFullscreen] = useState(false);
  const [previousSidebarTab, setPreviousSidebarTab] = useState<Exclude<SidebarTab, 'blackboard'> | null>(null);
  const [blackboardSeenMessageAt, setBlackboardSeenMessageAt] = useState(0);
  const [toolLogs, setToolLogs] = useState<ToolLog[]>([]);
  const [blackboardEvents, setBlackboardEvents] = useState<BlackboardEvent[]>([]);
  const [currentSessionId, setCurrentSessionId] = useState<string>(makeSessionId);
  const [sessions, setSessions] = useState<SessionMeta[]>([]);
  // Stores the latest plan report so subsequent code/debug/test skills get the
  // full architectural context from the planning discussion.
  const planReportRef = useRef<string>('');
  // Tracks the currently-executing invocation so the catch block can inject
  // precise retry context into Director's history when a skill fails.
  const lastInvocationRef = useRef<{ skill: AppMode; task: string; wsPath: string | null } | null>(null);
  const projectContextRef = useRef<string | null>(projectContext);
  projectContextRef.current = projectContext;
  const projectContextMetaRef = useRef<{ source: 'auto' | 'manual' | null; workspace: string | null }>({
    source: null,
    workspace: null,
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

  // Auto-show tool logs when new logs arrive
  useEffect(() => {
    if (toolLogs.length > 0 && activeSidebarTab === null) {
      setActiveSidebarTab('logs');
    }
  }, [toolLogs.length, activeSidebarTab]);

  // Auto-show explorer when workspace opens
  useEffect(() => {
    if (workspace && activeSidebarTab === null) {
      setActiveSidebarTab('explorer');
    }
  }, [workspace, activeSidebarTab]);

  // Keep project context bound to the workspace it came from so switching
  // projects does not silently reuse stale documentation.
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

  // When a workspace is set and no context has been manually uploaded,
  // auto-load any documentation files found in that directory.
  // This unifies "open project folder" and "upload document" into one path:
  // both ultimately populate projectContext, which skills read uniformly.
  useEffect(() => {
    if (!workspace || projectContext !== null) return;
    invoke<{ content: string; filenames: string[] }>('read_project_docs', { path: workspace })
      .then(docs => {
        if (docs.filenames.length > 0) {
          projectContextMetaRef.current = { source: 'auto', workspace };
          projectContextRef.current = docs.content;
          setProjectContext(docs.content);
        }
      })
      .catch(() => {});
  }, [workspace, projectContext]);

  // Keep the cached PLAN.md aligned with the active workspace so switching
  // projects never leaks an old plan into a new code / review / test run.
  useEffect(() => {
    let cancelled = false;

    if (!workspace) {
      planReportRef.current = '';
      return;
    }

    invoke<string>('read_workspace_file', {
      path: workspace,
      relativePath: 'PLAN.md',
    })
      .then(plan => {
        if (!cancelled) {
          planReportRef.current = plan;
        }
      })
      .catch(() => {
        if (!cancelled) {
          planReportRef.current = '';
        }
      });

    return () => {
      cancelled = true;
    };
  }, [workspace]);

  // ── Session history ────────────────────────────────────────────────────────

  // Reload session list whenever workspace changes
  useEffect(() => {
    invoke<SessionMeta[]>('list_sessions', { workspace }).then(setSessions).catch(() => {});
  }, [workspace]);

  const { flushPendingSessionSave, handleLoadSession, handleDeleteSession } = useSessionManager({
    workspace, currentSessionId, messages, sessions,
    messagesRef, toolLogsRef, blackboardEventsRef, projectContextRef,
    projectContextMetaRef, sessionIdRef, planReportRef,
    setMessages, setToolLogs, setBlackboardEvents, setCurrentSessionId,
    setSessions, setWorkspace, setProjectContext,
  });

  // ── Init ──────────────────────────────────────────────────────────────────

  const runDetection = useCallback(async () => {
    setChecking(true);
    try {
      const [sysStatus, cfgStatus] = await Promise.all([
        invoke<SystemStatus>('detect_tools'),
        invoke<ConfigStatus>('get_config'),
      ]);
      setStatus(sysStatus);
      setConfigStatus(cfgStatus);
    } catch (err) {
      console.error('Init error:', err);
      setStatus({
        claude: { installed: false, version: null, path: null },
        codex: { installed: false, version: null, path: null }
      });
      setConfigStatus({
        configured: false,
        base_url: '',
        model: '',
        api_format: 'openai',
        api_key_hint: '',
        vendored_skills: true,
        max_parallel_subtasks: 5,
        execution_access_mode: 'sandbox',
      });
    } finally {
      setChecking(false);
    }
  }, []);

  useEffect(() => { runDetection(); }, [runDetection]);

  // ── Message helpers ────────────────────────────────────────────────────────

  const addMessage = (role: ChatMessage['role'], content: string, thinking = false): string => {
    const id = makeId();
    setMessages((prev) => [...prev, { id, role, content, timestamp: Date.now(), thinking }]);
    return id;
  };

  const updateMessage = (id: string, content: string, thinking = false) => {
    setMessages((prev) => prev.map((m) => m.id === id ? { ...m, content, thinking } : m));
  };

  const pauseDirectorLoop = async (reason: string) => {
    const last = lastInvocationRef.current;
    const retryInstruction = last
      ? `请重新调用 ${last.skill} 技能（任务："${last.task}"${last.wsPath ? `，工作目录：${last.wsPath}` : ''}）从头执行。`
      : '请重新调用刚才失败的技能。';

    try {
      const currentHistory = await invoke<unknown[]>('get_director_history');
      await invoke('restore_director_history', {
        history: [
          ...currentHistory,
          {
            role: 'user',
            content: `[系统通知] 技能执行中断，错误：${reason}。任务已暂停，等待用户指令。用户告知恢复后，${retryInstruction}`,
          },
          {
            role: 'assistant',
            content: `了解。${last ? `${last.skill} 技能` : '上一个技能'}执行失败或超出轮数预算，任务暂停。等待用户确认后我会重新调用。`,
          },
        ],
      });
    } catch {
      // History injection failed — UI message is still shown
    }

    addMessage(
      'director',
      `⏸ 任务已暂停\n\n**原因**：${reason}\n\n等你确认后，告诉我"已经恢复了"，我会重新执行 ${last ? `\`${last.skill}\`` : '上一个'} 技能。`
    );
  };

  // ── Submit handler (Director loop) ─────────────────────────────────────────
  //
  // Each iteration: Director speaks → maybe invokes a skill → skill runs →
  // system notifies Director of completion → Director decides next step.
  // Max 5 rounds to prevent infinite loops.

  const handleSubmit = async (text: string) => {
    if (isRunning) return;
    stopRequestedRef.current = false;
    setIsRunning(true);
    addMessage('user', text);

    let nextInput = projectContextRef.current
      ? `用户已提供项目文档，plan 技能将以文档审阅模式运行（Claude 和 Codex 审阅文档并改写）。\n\n【任务】${text}`
      : text;

    let currentWsPath: string | null = workspaceRef.current;

    try {
      // Allow one full remediation / re-validation loop after QA while still
      // keeping runaway Director chains bounded.
      const MAX_ROUNDS = 16;
      let hitRoundBudget = true;
      for (let round = 0; round < MAX_ROUNDS; round++) {
        // ── Ask Director ────────────────────────────────────────────────────
        const replyId = addMessage('director', '', true);
        let accumulated = '';

        const unlisten = await appWindow.listen<string>('director-chat-chunk', (event) => {
          accumulated += event.payload;
          updateMessage(replyId, stripInvoke(accumulated), false);
        });

        try {
          await invoke('director_chat', { input: nextInput });
        } catch (err) {
          updateMessage(replyId, `错误：${String(err)}`, false);
          throw err; // stop the loop — caught by outer try/catch
        } finally {
          unlisten();
        }

        // ── Check for skill invocation ──────────────────────────────────────
        const invocation = parseInvoke(accumulated);
        if (!invocation) {
          if (stopRequestedRef.current) {
            stopRequestedRef.current = false;
            await pauseDirectorLoop('用户手动停止了当前任务。');
          }
          hitRoundBudget = false;
          break;
        }

        setCurrentMode(invocation.skill);
        lastInvocationRef.current = { skill: invocation.skill, task: invocation.task, wsPath: currentWsPath };

        if (invocation.skill === 'review') {
          const reviewResult = await runReview(invocation.task, currentWsPath);
          nextInput = buildNextInputAfterReview(reviewResult);
        } else if (invocation.skill === 'test') {
          const testPassed = await runTest(invocation.task, currentWsPath);
          if (!testPassed) {
            hitRoundBudget = false;
            break;
          }
          nextInput = await buildNextInputAfterTestWithEvidence(currentWsPath);
        } else if (invocation.skill === 'qa') {
          const qaResult = await runQa(invocation.task, currentWsPath);
          nextInput = await buildNextInputAfterQaWithEvidence(qaResult, currentWsPath);
        } else {
          const result = await runSkill(invocation.skill, invocation.task);
          if (result === null) {
            // runSkill already showed an error message — stop the loop
            hitRoundBudget = false;
            break;
          }
          currentWsPath = result;
          nextInput = await buildNextInputAfterCodeWithEvidence(invocation.skill, currentWsPath);
        }
      }

      if (hitRoundBudget) {
        await pauseDirectorLoop(`Director exceeded the round budget (${MAX_ROUNDS}) before the task reached a stable stop condition.`);
      }
    } catch (err) {
      const errStr = (err && typeof err === 'object' && 'message' in err) ? (err as { message: string }).message : String(err);
      const isCancelled = stopRequestedRef.current || errStr === 'cancelled'
        || (err && typeof err === 'object' && 'kind' in err && (err as { kind: string }).kind === 'cancelled');
      const reason = isCancelled
        ? '用户手动停止了当前任务。'
        : errStr;
      stopRequestedRef.current = false;
      await pauseDirectorLoop(reason);
    } finally {
      stopRequestedRef.current = false;
      setCurrentMode(null);
      setIsRunning(false);
      setIsStopping(false);
    }
  };

  const { runReview, runTest, runQa, runSkill, handleStop } = createSkillRunner({
    workspaceRef, projectContextRef, projectContextMetaRef, planReportRef, stopRequestedRef,
    addMessage, updateMessage,
    setCurrentMode, setToolLogs, setBlackboardEvents, setMessages, setWorkspace, setIsStopping,
    isRunning, isStopping,
  });

  const handleOpenProject = useCallback(async () => {
    const selected = await openDialog({ directory: true, multiple: false, title: '选择项目文件夹' });
    if (!selected) return;
    try {
      const validated = await invoke<string>('open_project', { path: selected as string });
      const switchingWorkspace = workspaceRef.current !== validated;
      if (switchingWorkspace) {
        await flushPendingSessionSave();
      }
      const meta = projectContextMetaRef.current;
      if (meta.workspace && meta.workspace !== validated) {
        projectContextMetaRef.current = { source: null, workspace: null };
        projectContextRef.current = null;
        setProjectContext(null);
      }
      if (switchingWorkspace) {
        setMessages([]);
        setToolLogs([]);
        setBlackboardEvents([]);
        planReportRef.current = '';
        syncSessionIdentity(makeSessionId(), sessionIdRef, setCurrentSessionId);
        await invoke('clear_history');
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
    setBlackboardEvents([]);
    planReportRef.current = '';
    syncSessionIdentity(makeSessionId(), sessionIdRef, setCurrentSessionId);
    await invoke('clear_history');
  }, [flushPendingSessionSave]);

  const handleNewWindow = useCallback(() => {
    invoke('open_new_window').catch(console.error);
  }, []);

  const handleOpenConfigEditor = useCallback(async () => {
    setConfigError(null);
    setConfigDraft(null);
    setShowConfigEditor(true);
    try {
      const draft = await invoke<ConfigDraft>('get_config_form');
      setConfigDraft(draft);
    } catch (err) {
      setConfigDraft(null);
      setConfigError(String(err));
    }
  }, []);

  const handleSaveConfig = useCallback(async () => {
    if (!configDraft || configSaving) return;
    setConfigSaving(true);
    setConfigError(null);
    try {
      const status = await invoke<ConfigStatus>('save_config', { config: configDraft });
      setConfigStatus(status);
      setShowConfigEditor(false);
    } catch (err) {
      setConfigError(String(err));
    } finally {
      setConfigSaving(false);
    }
  }, [configDraft, configSaving]);

  const handleToggleExecutionAccess = useCallback(async (mode: ConfigStatus['execution_access_mode']) => {
    if (configUpdating) return;
    setConfigUpdating(true);
    setConfigError(null);
    try {
      const status = await invoke<ConfigStatus>('set_execution_access_mode', { mode });
      setConfigStatus(status);
      setConfigDraft(prev => (prev ? { ...prev, execution_access_mode: mode } : prev));
    } catch (err) {
      setConfigError(String(err));
    } finally {
      setConfigUpdating(false);
    }
  }, [configUpdating]);

  const latestAgentMessageAt = messages.reduce((latest, message) => {
    if (message.role === 'user') return latest;
    return Math.max(latest, message.timestamp);
  }, 0);

  const unreadAgentMessages = blackboardFullscreen
    ? messages.filter(message => message.role !== 'user' && message.timestamp > blackboardSeenMessageAt).length
    : 0;

  const closeBlackboardWorkspace = useCallback(() => {
    setBlackboardSeenMessageAt(latestAgentMessageAt);
    setBlackboardFullscreen(false);
    setActiveSidebarTab(previousSidebarTab);
  }, [latestAgentMessageAt, previousSidebarTab]);

  const openBlackboardWorkspace = useCallback(() => {
    setPreviousSidebarTab(activeSidebarTab && activeSidebarTab !== 'blackboard' ? activeSidebarTab : null);
    setBlackboardSeenMessageAt(latestAgentMessageAt);
    setBlackboardFullscreen(true);
    setActiveSidebarTab('blackboard');
  }, [activeSidebarTab, latestAgentMessageAt]);

  const toggleSidebarTab = useCallback((tab: Exclude<SidebarTab, 'blackboard'>) => {
    setBlackboardFullscreen(false);
    setActiveSidebarTab(prev => prev === tab ? null : tab);
  }, []);

  const toggleBlackboardWorkspace = useCallback(() => {
    if (blackboardFullscreen && activeSidebarTab === 'blackboard') {
      closeBlackboardWorkspace();
      return;
    }
    openBlackboardWorkspace();
  }, [activeSidebarTab, blackboardFullscreen, closeBlackboardWorkspace, openBlackboardWorkspace]);

  // ── Render ─────────────────────────────────────────────────────────────────

  const modeLabel = currentMode
    ? (MODES.find((m) => m.id === currentMode)?.label ?? currentMode)
    : null;
  const sidebarWidth = activeSidebarTab === null || blackboardFullscreen
    ? '0px'
    : activeSidebarTab === 'blackboard'
      ? 'min(62vw, 680px)'
      : '280px';

  return (
    <ThemeProvider>
      <ModeActivated mode={currentMode} />
      {/* Global Background Layer with glowing orbs for Glassmorphism effect */}
      <div className="fixed inset-0 z-0 bg-background-light dark:bg-background-dark pointer-events-none overflow-hidden">
        <div className="absolute top-[-10%] left-[-10%] w-[40%] h-[40%] rounded-full bg-violet-400/20 dark:bg-violet-600/20 blur-[100px] animate-blob" />
        <div className="absolute bottom-[-10%] right-[-10%] w-[40%] h-[40%] rounded-full bg-blue-400/20 dark:bg-blue-600/20 blur-[100px] animate-blob animation-delay-2000" />
        <div className="absolute top-[20%] right-[20%] w-[30%] h-[30%] rounded-full bg-rose-400/20 dark:bg-rose-600/20 blur-[100px] animate-blob animation-delay-4000" />
      </div>

      <div className="flex flex-col h-screen w-screen overflow-hidden bg-transparent font-sans animate-app-entrance relative z-10 text-zinc-800 dark:text-zinc-100">

        {/* Top bar / Custom Window Titlebar */}
        <header
          className="flex items-center justify-between pr-5 pl-24 h-12 flex-shrink-0
                     glass-header relative z-50 select-none"
        >
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

          {/* Central explicit drag region occupying all empty space */}
          <div data-tauri-drag-region className="flex-1 h-full self-stretch drag-region" style={{ WebkitAppRegion: 'drag' } as React.CSSProperties} />

          <div className="flex items-center gap-1.5 relative z-50 pointer-events-auto shrink-0">
            <button
              onClick={handleOpenConfigEditor}
              className="flex h-8 items-center gap-1.5 rounded-lg px-2.5 text-[11px] text-zinc-500 transition-all
                         hover:bg-zinc-200/50 hover:text-zinc-700
                         dark:text-zinc-400 dark:hover:bg-zinc-800/50 dark:hover:text-zinc-200"
              title="Settings"
            >
              <VscSettingsGear className="h-3.5 w-3.5" />
              <span className="hidden sm:inline font-medium">Settings</span>
            </button>
            <button
              onClick={() => { setContextDraft(projectContext ?? ''); setShowContextEditor(true); }}
              className={`flex h-8 items-center gap-1.5 rounded-lg px-2.5 text-[11px] transition-all
                         hover:bg-zinc-200/50 dark:hover:bg-zinc-800/50
                         ${projectContext
                  ? 'text-violet-600 dark:text-violet-400'
                  : 'text-zinc-500 dark:text-zinc-400 hover:text-zinc-700 dark:hover:text-zinc-200'}`}
              title="Project Context"
            >
              <svg className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth={1.5} viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m2.25 0H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z" />
              </svg>
              {projectContext && (
                <span className="hidden sm:inline font-medium">
                  {(projectContext.length / 1024).toFixed(1)}K
                </span>
              )}
            </button>
            <button
              onClick={handleNewWindow}
              className="flex h-8 items-center gap-1.5 rounded-lg px-2.5 text-[11px] text-zinc-500 transition-all
                         hover:bg-zinc-200/50 hover:text-zinc-700
                         dark:text-zinc-400 dark:hover:bg-zinc-800/50 dark:hover:text-zinc-200"
              title="New Window"
            >
              <VscMultipleWindows className="w-3.5 h-3.5" />
            </button>
            <div className="h-4 w-px bg-zinc-200 dark:bg-zinc-800" />
            <div className="rounded-lg transition-all hover:bg-zinc-200/50 dark:hover:bg-zinc-800/50">
              <ThemeToggle />
            </div>
          </div>
        </header>

        {/* Main area: Integrated Activity Bar + Sidebar + Chat */}
        <div className="flex-1 flex min-h-0 bg-transparent overflow-hidden relative">

          {/* Activity Bar */}
          <div className="w-12 h-full flex flex-col items-center py-3 gap-1 glass-container border-r border-zinc-200/50 dark:border-zinc-800/50 z-30 flex-shrink-0">
            {[
              { tab: 'explorer' as const, icon: <VscFiles className="w-[18px] h-[18px]" />, title: 'Explorer', badge: false },
              { tab: 'logs' as const, icon: <VscTerminal className="w-[18px] h-[18px]" />, title: 'Tool Logs', badge: toolLogs.length > 0 && activeSidebarTab !== 'logs' },
              { tab: 'history' as const, icon: <VscHistory className="w-[18px] h-[18px]" />, title: 'History', badge: sessions.length > 0 && activeSidebarTab !== 'history' },
            ].map(({ tab, icon, title, badge }) => (
              <button
                key={tab}
                onClick={() => toggleSidebarTab(tab)}
                className={`relative flex h-9 w-9 items-center justify-center rounded-xl transition-all duration-200 ${
                  activeSidebarTab === tab
                    ? 'bg-white text-violet-600 shadow-sm ring-1 ring-zinc-200/60 dark:bg-zinc-800 dark:text-violet-400 dark:ring-zinc-700/50'
                    : 'text-zinc-400 hover:bg-zinc-200/40 hover:text-zinc-700 dark:text-zinc-500 dark:hover:bg-zinc-800/40 dark:hover:text-zinc-300'
                }`}
                title={title}
              >
                {icon}
                {badge && (
                  <span className="absolute -right-0.5 -top-0.5 h-2 w-2 rounded-full bg-blue-500 ring-[1.5px] ring-white shadow-[0_0_6px_rgba(59,130,246,0.5)] dark:ring-zinc-950" />
                )}
              </button>
            ))}

            <div className="my-1 h-px w-5 bg-zinc-200/60 dark:bg-zinc-800/60" />

            <button
              onClick={toggleBlackboardWorkspace}
              className={`relative flex h-9 w-9 items-center justify-center rounded-xl transition-all duration-200 ${
                activeSidebarTab === 'blackboard'
                  ? 'bg-white text-violet-600 shadow-sm ring-1 ring-zinc-200/60 dark:bg-zinc-800 dark:text-violet-400 dark:ring-zinc-700/50'
                  : 'text-zinc-400 hover:bg-zinc-200/40 hover:text-zinc-700 dark:text-zinc-500 dark:hover:bg-zinc-800/40 dark:hover:text-zinc-300'
              }`}
              title="Blackboard"
            >
              <VscChecklist className="w-[18px] h-[18px]" />
              {(blackboardEvents.length > 0 || unreadAgentMessages > 0) && activeSidebarTab !== 'blackboard' && (
                <span className="absolute -right-0.5 -top-0.5 h-2 w-2 rounded-full bg-violet-500 ring-[1.5px] ring-white shadow-[0_0_6px_rgba(139,92,246,0.5)] dark:ring-zinc-950" />
              )}
            </button>
          </div>

          {/* Docked Sidebar Container */}
          <div
            className={`h-full border-r border-zinc-200/50 dark:border-zinc-800/50 glass-container flex-shrink-0 overflow-hidden transition-[width] duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] z-20 ${activeSidebarTab !== null && !blackboardFullscreen
                ? 'opacity-100'
                : 'opacity-0 border-none'
              }`}
            style={{ width: sidebarWidth }}
          >
            <div className="h-full relative" style={{ width: sidebarWidth }}>
              <div className={`absolute inset-0 transition-opacity duration-300 ${activeSidebarTab === 'explorer' ? 'opacity-100 z-10 pointer-events-auto' : 'opacity-0 z-0 pointer-events-none'}`}>
                <FileTreePanel
                  workspacePath={workspace}
                  onOpenProject={handleOpenProject}
                  onClose={() => setActiveSidebarTab(null)}
                />
              </div>
              <div className={`absolute inset-0 transition-opacity duration-300 ${activeSidebarTab === 'logs' ? 'opacity-100 z-10 pointer-events-auto' : 'opacity-0 z-0 pointer-events-none'}`}>
                <ToolLogPanel
                  logs={toolLogs}
                  onClose={() => setActiveSidebarTab(null)}
                />
              </div>
              <div className={`absolute inset-0 transition-opacity duration-300 ${activeSidebarTab === 'history' ? 'opacity-100 z-10 pointer-events-auto' : 'opacity-0 z-0 pointer-events-none'}`}>
                <HistoryPanel
                  sessions={sessions}
                  currentSessionId={currentSessionId}
                  onLoad={handleLoadSession}
                  onDelete={handleDeleteSession}
                  onNewChat={handleNewChat}
                  onClose={() => setActiveSidebarTab(null)}
                />
              </div>
              <div className={`absolute inset-0 transition-opacity duration-300 ${activeSidebarTab === 'blackboard' && !blackboardFullscreen ? 'opacity-100 z-10 pointer-events-auto' : 'opacity-0 z-0 pointer-events-none'}`}>
                <BlackboardPanel
                  workspacePath={workspace}
                  events={blackboardEvents}
                  onClose={() => setActiveSidebarTab(null)}
                />
              </div>
            </div>
          </div>

          {/* Main Content Area */}
          {blackboardFullscreen ? (
            <div className="flex-1 min-w-0 z-10 basis-0 bg-transparent shadow-[-4px_0_15px_-5px_rgba(0,0,0,0.05)] dark:shadow-[-4px_0_15px_-5px_rgba(0,0,0,0.4)]">
              <BlackboardPanel
                workspacePath={workspace}
                events={blackboardEvents}
                onClose={closeBlackboardWorkspace}
                onBack={closeBlackboardWorkspace}
                fullscreen
                unreadAgentMessages={unreadAgentMessages}
              />
            </div>
          ) : (
            <div className="flex flex-1 min-h-0 min-w-0 basis-0 flex-col overflow-hidden relative z-10 bg-transparent shadow-[-4px_0_15px_-5px_rgba(0,0,0,0.05)] dark:shadow-[-4px_0_15px_-5px_rgba(0,0,0,0.4)]">
              <ChatPanel messages={messages} onOpenProject={handleOpenProject} workspace={workspace} />
              <div className="absolute bottom-0 left-0 right-0 p-4 sm:p-6
                              bg-gradient-to-t from-background-light via-background-light/80 to-transparent
                              dark:from-background-dark dark:via-background-dark/80
                              pointer-events-none z-10 flex flex-col justify-end">
                <div className="pointer-events-auto max-w-4xl w-full mx-auto">
                  <InputBar
                    mode={currentMode ?? 'chat'}
                    status={status}
                  configStatus={configStatus}
                  isRunning={isRunning}
                  isStopping={isStopping}
                  configUpdating={configUpdating}
                  onSubmit={handleSubmit}
                  onStop={handleStop}
                  onToggleExecutionAccess={handleToggleExecutionAccess}
                />
                </div>
              </div>
            </div>
          )}
        </div>

      </div>
      {showConfigEditor && (
        <ConfigEditorModal
          draft={configDraft}
          status={status}
          checking={checking}
          saving={configSaving}
          error={configError}
          onClose={() => {
            if (configSaving) return;
            setShowConfigEditor(false);
            setConfigError(null);
          }}
          onChange={setConfigDraft}
          onSave={handleSaveConfig}
          onRecheckEnvironment={runDetection}
        />
      )}
      {/* Project Context Editor Modal */}
      {showContextEditor && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-white/20 backdrop-blur-xl dark:bg-zinc-950/60"
          onClick={() => setShowContextEditor(false)}
          onKeyDown={(e) => { if (e.key === 'Escape') setShowContextEditor(false); }}
        >
          <div
            className="mx-4 flex max-h-[80vh] w-full max-w-2xl flex-col overflow-hidden glass-panel"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-start justify-between gap-4 border-b border-zinc-200/40 bg-white/10 px-5 py-4 dark:border-zinc-800/50 dark:bg-zinc-900/25">
              <div>
                <h2 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200">Project Context</h2>
                <p className="text-xs text-zinc-500 mt-0.5">粘贴开发文档，Claude & Codex 编码时将以此为依据</p>
              </div>
              <button
                onClick={() => setShowContextEditor(false)}
                className="rounded-lg p-1 text-zinc-400 transition-colors hover:bg-zinc-100 hover:text-zinc-600 dark:hover:bg-zinc-800 dark:hover:text-zinc-300"
                title="关闭 (Esc)"
              >
                <svg className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            <textarea
              value={contextDraft}
              onChange={(e) => setContextDraft(e.target.value)}
              placeholder="粘贴你的开发文档、需求说明、技术规范..."
              className="flex-1 bg-transparent p-4 text-sm font-mono text-zinc-800 dark:text-zinc-200
                         border-none resize-none focus:outline-none min-h-[300px]
                         placeholder-zinc-400 dark:placeholder-zinc-600"
              autoFocus
            />
            <div className="flex items-center justify-between border-t border-zinc-200/40 bg-white/10 px-5 py-3 dark:border-zinc-800/50 dark:bg-zinc-900/25">
              <button
                onClick={() => {
                  projectContextMetaRef.current = { source: null, workspace: null };
                  projectContextRef.current = null;
                  setProjectContext(null);
                  setContextDraft('');
                  setShowContextEditor(false);
                }}
                className="text-xs text-rose-500 hover:text-rose-600 transition-colors font-medium"
              >
                清除文档
              </button>
              <div className="flex gap-2">
                <button
                  onClick={() => setShowContextEditor(false)}
                  className="text-xs px-4 py-1.5 rounded-lg glass-button text-zinc-600 dark:text-zinc-300 font-medium"
                >
                  取消
                </button>
                <button
                  onClick={() => {
                    const next = contextDraft.trim() || null;
                    projectContextMetaRef.current = next
                      ? { source: 'manual', workspace: workspaceRef.current }
                      : { source: null, workspace: null };
                    projectContextRef.current = next;
                    setProjectContext(next);
                    setShowContextEditor(false);
                  }}
                  disabled={!contextDraft.trim()}
                  className="text-xs px-4 py-1.5 rounded-lg bg-violet-600/90 text-white shadow-md shadow-violet-500/20 backdrop-blur-sm
                             hover:bg-violet-600 hover:shadow-lg hover:shadow-violet-500/30 font-medium disabled:opacity-50 disabled:cursor-not-allowed transition-all"
                >
                  应用文档
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </ThemeProvider>
  );
}
