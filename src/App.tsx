import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { open as openDialog } from '@tauri-apps/plugin-dialog';

// Scope all event listeners to the current window so multiple windows
// don't receive each other's skill-chunk / tool-log / director events.
const appWindow = getCurrentWebviewWindow();
import { AppMode, ChatMessage, ReviewPhaseResult, ToolLog, SystemStatus, ConfigStatus, MODES, SessionMeta, Session, BlackboardEvent } from './types';
import { ThemeProvider, useTheme } from './components/ThemeProvider';
import ModeActivated from './components/ModeActivated';
import StatusPanel from './components/StatusPanel';
import ChatPanel from './components/ChatPanel';
import InputBar from './components/InputBar';
import FileTreePanel from './components/FileTreePanel';
import ToolLogPanel from './components/ToolLogPanel';
import HistoryPanel from './components/HistoryPanel';
import BlackboardPanel from './components/BlackboardPanel';
import { VscColorMode, VscFiles, VscHistory, VscMultipleWindows, VscTerminal, VscChecklist } from 'react-icons/vsc';

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
import { makeId, makeSessionId } from './utils';

// ── App ────────────────────────────────────────────────────────────────────────

export default function App() {
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
  const [activeSidebarTab, setActiveSidebarTab] = useState<'explorer' | 'logs' | 'history' | 'blackboard' | null>(null);
  const [toolLogs, setToolLogs] = useState<ToolLog[]>([]);
  const [blackboardEvents, setBlackboardEvents] = useState<BlackboardEvent[]>([]);
  const [currentSessionId, setCurrentSessionId] = useState<string>(makeSessionId);
  const [sessions, setSessions] = useState<SessionMeta[]>([]);
  // Stores the latest plan report so subsequent code/debug/test skills get the
  // full architectural context from the planning discussion.
  const planReportRef = useRef<string>('');
  // Tracks the currently-executing invocation so the catch block can inject
  // precise retry context into Director's history when a skill fails.
  const lastInvocationRef = useRef<{ skill: AppMode; task: string; dir?: string; wsPath: string | null } | null>(null);
  const projectContextRef = useRef<string | null>(projectContext);
  projectContextRef.current = projectContext;
  const projectContextMetaRef = useRef<{ source: 'auto' | 'manual' | null; workspace: string | null }>({
    source: null,
    workspace: null,
  });

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

  // ── Session history ────────────────────────────────────────────────────────

  // Reload session list whenever workspace changes
  useEffect(() => {
    invoke<SessionMeta[]>('list_sessions', { workspace }).then(setSessions).catch(() => {});
  }, [workspace]);

  // Refs so auto-save closure always sees latest values without needing them as deps
  const sessionIdRef = useRef(currentSessionId);
  sessionIdRef.current = currentSessionId;
  const workspaceRef = useRef(workspace);
  workspaceRef.current = workspace;

  // Auto-save: debounced, fires 1.5s after any messages/toolLogs/blackboard changes.
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (messages.length === 0) return;
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    saveTimerRef.current = setTimeout(async () => {
      const title = messages.find(m => m.role === 'user')?.content.slice(0, 60) ?? '新对话';
      const ws = workspaceRef.current;
      try {
        const directorHistory = await invoke<unknown[]>('get_director_history');
        await invoke('save_session', {
          workspace: ws,
          session: {
            id: sessionIdRef.current,
            title,
            workspace_path: ws,
            created_at: messages[0].timestamp,
            updated_at: Date.now(),
            message_count: messages.length,
            messages,
            tool_logs: toolLogs,
            blackboard_events: blackboardEvents,
            project_context: projectContextRef.current,
            project_context_source: projectContextMetaRef.current.source,
            director_history: directorHistory,
          },
        });
        const list = await invoke<SessionMeta[]>('list_sessions', { workspace: ws });
        setSessions(list);
      } catch (err) {
        console.error('auto-save error:', err);
      }
    }, 1500);
    return () => { if (saveTimerRef.current) clearTimeout(saveTimerRef.current); };
  }, [messages, toolLogs, blackboardEvents]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleLoadSession = useCallback(async (sessionId: string) => {
    try {
      const s = await invoke<Session>('load_session', { workspace, sessionId });
      const restoredWorkspace = s.workspace_path ?? workspace;
      setMessages(s.messages);
      setToolLogs(s.tool_logs as ToolLog[]);
      setBlackboardEvents(s.blackboard_events || []);
      setCurrentSessionId(s.id);
      projectContextRef.current = s.project_context ?? null;
      setProjectContext(s.project_context ?? null);
      projectContextMetaRef.current = s.project_context
        ? {
          source: s.project_context_source === 'auto' || s.project_context_source === 'manual'
            ? s.project_context_source
            : 'manual',
          workspace: restoredWorkspace,
        }
        : { source: null, workspace: null };
      setWorkspace(restoredWorkspace);
      if (restoredWorkspace) {
        try {
          planReportRef.current = await invoke<string>('read_workspace_file', {
            path: restoredWorkspace,
            relativePath: 'PLAN.md',
          });
        } catch {
          planReportRef.current = '';
        }
      } else {
        planReportRef.current = '';
      }
      // Restore the Director's conversation history so it has full context
      // of the previous session — same behaviour as Cursor resuming a conversation.
      await invoke('restore_director_history', { history: s.director_history ?? [] });
    } catch (err) {
      console.error('load_session error:', err);
    }
  }, [workspace]);

  const handleDeleteSession = useCallback(async (sessionId: string) => {
    try {
      await invoke('delete_session', { workspace, sessionId });
      setSessions(prev => prev.filter(s => s.id !== sessionId));
      if (sessionId === currentSessionId) {
        setMessages([]);
        setToolLogs([]);
        setBlackboardEvents([]);
        planReportRef.current = '';
        setCurrentSessionId(makeSessionId());
        await invoke('clear_history');
      }
    } catch (err) {
      console.error('delete_session error:', err);
    }
  }, [workspace, currentSessionId]);

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
      setConfigStatus({ configured: false, base_url: '', model: '', api_format: 'openai', api_key_hint: '', vendored_skills: true });
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

  // ── Submit handler (Director loop) ─────────────────────────────────────────
  //
  // Each iteration: Director speaks → maybe invokes a skill → skill runs →
  // system notifies Director of completion → Director decides next step.
  // Max 5 rounds to prevent infinite loops.

  const handleSubmit = async (text: string) => {
    if (isRunning) return;
    setIsRunning(true);
    addMessage('user', text);

    let nextInput = projectContextRef.current
      ? `用户已提供项目文档，plan 技能将以文档审阅模式运行（Claude 和 Codex 审阅文档并改写）。\n\n【任务】${text}`
      : text;

    let currentWsPath: string | null = workspaceRef.current;

    try {
      const MAX_ROUNDS = 8;
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
        if (!invocation) break;

        setCurrentMode(invocation.skill);
        lastInvocationRef.current = { skill: invocation.skill, task: invocation.task, dir: invocation.dir, wsPath: currentWsPath };

        if (invocation.skill === 'review') {
          const reviewResult = await runReview(invocation.task, currentWsPath);
          if (reviewResult.securityFailed) {
            // Security audit found critical issues: write security.md, then re-enter code mode to fix
            nextInput = `Review 安全审查发现严重安全问题，已生成 security.md 报告。\n\n安全问题摘要：${reviewResult.securityIssue}\n\n请立即调用 code 技能，按照 security.md 中记录的问题逐一修复，修复后在 security.md 中标记每项问题为已解决。`;
          } else {
            nextInput = `review 已完成：安全审查 ✓、代码清理 ✓。请立即调用 test 技能对项目进行完整集成测试（启动服务器 + curl 测试所有接口）。`;
          }
        } else if (invocation.skill === 'test') {
          await runTest(invocation.task, currentWsPath);
          nextInput = `test 集成测试及项目报告已完成。请用一句话总结结果并结束任务。`;
        } else {
          const result = await runSkill(invocation.skill, invocation.task, invocation.dir);
          if (result === null) {
            // runSkill already showed an error message — stop the loop
            break;
          }
          currentWsPath = result;
          if (invocation.skill === 'plan') {
            nextInput = `plan 技能已完成：Claude 完成了 5 轮规划讨论，并将完整架构文档（PLAN.md）写入了项目目录。请用一句话简要说明最终技术方案，然后立即调用 code 技能按照 PLAN.md 开始开发。`;
          } else {
            nextInput = `${invocation.skill} 技能已完成。code 模式中的功能级 review 已按子任务执行完毕。请立即调用 review 进行最终安全审查和代码清理。`;
          }
        }
      }
    } catch (err) {
      // Inject failure context into Director's conversation history so it knows
      // exactly what to retry when the user says "已经恢复了".
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
              content: `[系统通知] 技能执行中断，错误：${String(err)}。任务已暂停，等待用户指令。用户告知恢复后，${retryInstruction}`,
            },
            {
              role: 'assistant',
              content: `了解。${last ? `${last.skill} 技能` : '上一个技能'}执行失败，任务暂停。等待用户确认后我会重新调用。`,
            },
          ],
        });
      } catch {
        // History injection failed — UI message is still shown
      }
      addMessage('director',
        `⏸ 任务已暂停\n\n**原因**：${String(err)}\n\n等 CLI 恢复后，告诉我"已经恢复了"，我会重新执行 ${last ? `\`${last.skill}\`` : '上一个'} 技能。`
      );
    } finally {
      setCurrentMode(null);
      setIsRunning(false);
      setIsStopping(false);
    }
  };

  // ── Shared phase runner (review & test both use phased execution) ──────────

  const runPhase = async (
    mode: AppMode,
    phase: string,
    task: string,
    wsPath: string | null,
    issue?: string,
    contextOverride?: string | null
  ): Promise<ReviewPhaseResult> => {
    const agentMsgIds = new Map<string, string>();
    const agentContent = new Map<string, string>();

    const unlistenChunks = await appWindow.listen<{ agent: string; text: string; reset: boolean }>('skill-chunk', (event) => {
      const { agent, text, reset } = event.payload;
      const role = agent as ChatMessage['role'];
      if (reset || !agentMsgIds.has(agent)) {
        agentMsgIds.set(agent, addMessage(role, ''));
        agentContent.set(agent, '');
      }
      const id = agentMsgIds.get(agent)!;
      const updated = agentContent.get(agent)! + text;
      agentContent.set(agent, updated);
      updateMessage(id, updated);
    });

    let result: ReviewPhaseResult = { phase, passed: true, issue: '' };
    const unlistenResult = await appWindow.listen<ReviewPhaseResult>('review-phase-result', (event) => {
      result = event.payload;
    });
    const unlistenToolLog = await appWindow.listen<ToolLog>('tool-log', (event) => {
      setToolLogs(prev => [...prev, event.payload]);
    });
    const unlistenBlackboard = await appWindow.listen<BlackboardEvent>('blackboard-updated', (event) => {
      setBlackboardEvents(prev => [...prev, event.payload]);
      setMessages(prev => [...prev, {
        id: makeId(),
        role: 'director',
        content: `📌 ${event.payload.summary}`,
        timestamp: Date.now(),
      }]);
    });
    const unlistenCompletionReport = await appWindow.listen<string>('completion-report', (event) => {
      setMessages(prev => [...prev, {
        id: makeId(), role: 'director', content: event.payload,
        timestamp: Date.now(), isReport: true,
      }]);
    });

    try {
      await invoke('run_skill', {
        mode, task, workspace: wsPath, phase,
        context: contextOverride !== undefined ? contextOverride : projectContextRef.current,
        issue: issue ?? null,
      });
    } finally {
      unlistenChunks();
      unlistenResult();
      unlistenToolLog();
      unlistenBlackboard();
      unlistenCompletionReport();
    }
    return result;
  };

  // ── Review: final audit only (security + cleanup) ───────────────────────────
  //
  // Returns:
  //   securityFailed  → security audit found critical issues → write security.md, re-enter code mode
  //   (otherwise)     → audit passed → proceed to test mode

  const runReview = async (
    task: string,
    wsPath: string | null
  ): Promise<{ securityFailed?: boolean; securityIssue?: string }> => {
    setCurrentMode('review');

    // ── Phase 1: Security ──────────────────────────────────────────────────
    addMessage('director', '🔐 Review 1/2 — Security Audit');
    const sec = await runPhase('review', 'security', task, wsPath);

    // ── Phase 2: Code Cleanup (dead code, unused imports) — always runs ────
    addMessage('director', '🧹 Review 2/2 — Code Cleanup');
    await runPhase('review', 'cleanup', task, wsPath);

    setCurrentMode('chat');

    if (!sec.passed) {
      addMessage('director', `⛔ Critical security issue: ${sec.issue}. Security report generated — please fix before testing.`);
      return { securityFailed: true, securityIssue: sec.issue };
    }

    return {};
  };

  // ── Test: full integration test (env setup + curl suite + fix + document) ──

  const runTest = async (task: string, wsPath: string | null) => {
    setCurrentMode('test');

    // Combine plan architecture report + project docs so Claude knows
    // exactly what was planned and what should be tested.
    const testContext = planReportRef.current
      ? `## 技术方案 / 计划功能（请以此为测试 checklist 基准）\n\n${planReportRef.current}\n\n---\n\n${projectContextRef.current ?? ''}`.trimEnd()
      : projectContextRef.current;

    const phase = (p: string, issue?: string) =>
      runPhase('test', p, task, wsPath, issue, testContext);

    // ── Phase 0: Generate test.md (Claude + Codex parallel) ───────────────
    addMessage('director', '📝 Test 1/4 — Generating test plan (Claude + Codex 并行根据 PLAN.md 商讨测试方案...)');
    await phase('gen_test_plan');

    // ── Phase 1: Frontend tests ────────────────────────────────────────────
    addMessage('director', '🌐 Test 2/4 — Frontend Testing (浏览器自动化测试 UI...)');
    const frontendResult = await phase('frontend_test');
    if (!frontendResult.passed) {
      addMessage('director', `⚠️ 前端测试发现问题：${frontendResult.issue}，已写入 bugs.md，继续后端测试...`);
    }

    // ── Phase 2: Integration test suite ───────────────────────────────────
    addMessage('director', '🧪 Test 3/4 — Integration Testing (启动服务器 + curl 全量接口测试...)');
    let testResult = await phase('integration_test');

    if (!testResult.passed) {
      // bugs.md has been written by integration_test phase — pass its path as context
      const bugsNote = `bugs.md 已在工作目录中记录所有失败用例，请逐条修复。失败摘要：${testResult.issue}`;
      addMessage('director', `⚠️ 测试失败，已生成 bugs.md。正在让 Claude 逐条修复...`);
      const fix = await phase('fix', bugsNote);

      if (fix.passed) {
        addMessage('director', '🔄 Claude 完成修复，重新运行测试...');
        testResult = await phase('integration_test');
      }

      if (!testResult.passed) {
        addMessage('director', '📡 升级到 Codex 处理剩余问题...');
        const codexFix = await phase('codex_fix', `bugs.md 中仍有未解决项。摘要：${testResult.issue}`);

        if (codexFix.passed) {
          addMessage('director', '🔄 Codex 完成修复，运行最终测试...');
          testResult = await phase('integration_test');
        }

        if (!testResult.passed) {
          addMessage('director', `📋 自动修复已穷尽，bugs.md 中仍有未解决项。任务暂停，请人工介入。`);
        }
      }
    }

    // ── Phase 3: Completion document (always runs) ─────────────────────────
    addMessage('director', '📄 Test 4/4 — Generating Project Completion Report...');
    await phase('document');

    setCurrentMode('chat');
  };

  // ── Skills ─────────────────────────────────────────────────────────────────

  // Returns the workspace path used/created during the skill run so the Director
  // loop can pass it to subsequent skills (e.g. review).
  const runSkill = async (mode: AppMode, task: string, _dir?: string): Promise<string | null> => {
    // plan: Rust creates the workspace at skill start and broadcasts "plan-workspace".
    // code/debug/review/test: MUST reuse the workspace established by plan.
    //   Never create a new directory — that would scatter files across different folders.
    let wsPath: string | null = workspaceRef.current;
    if (mode !== 'plan' && !wsPath) {
      // No workspace yet and this isn't a plan run — surface the problem clearly
      // instead of silently creating an unrelated directory.
      addMessage('director', '⚠️ 没有工作目录。请先运行 plan 技能建立项目目录，再执行 code / review / test。');
      return null;
    }

    const agentMsgIds = new Map<string, string>();
    const agentContent = new Map<string, string>();

    const unlistenChunks = await appWindow.listen<{ agent: string; text: string; reset: boolean }>('skill-chunk', (event) => {
      const { agent, text, reset } = event.payload;
      const role = agent as ChatMessage['role'];
      if (reset || !agentMsgIds.has(agent)) {
        agentMsgIds.set(agent, addMessage(role, ''));
        agentContent.set(agent, '');
      }
      const id = agentMsgIds.get(agent)!;
      const updated = agentContent.get(agent)! + text;
      agentContent.set(agent, updated);
      updateMessage(id, updated);
    });

    let reportContent = '';
    const unlistenReport = await appWindow.listen<string>('plan-report', (event) => {
      reportContent = event.payload;
    });
    // plan.rs emits the workspace it created (or confirmed) after synthesis
    const unlistenPlanWs = await appWindow.listen<string>('plan-workspace', (event) => {
      wsPath = event.payload;
      const meta = projectContextMetaRef.current;
      if (meta.source === 'manual' && meta.workspace === null && projectContextRef.current !== null) {
        projectContextMetaRef.current = { ...meta, workspace: event.payload };
      }
      setWorkspace(event.payload);
    });
    const unlistenToolLog = await appWindow.listen<ToolLog>('tool-log', (event) => {
      setToolLogs(prev => [...prev, event.payload]);
    });
    const unlistenBlackboard = await appWindow.listen<BlackboardEvent>('blackboard-updated', (event) => {
      setBlackboardEvents(prev => [...prev, event.payload]);
      setMessages(prev => [...prev, {
        id: makeId(),
        role: 'director',
        content: `📌 ${event.payload.summary}`,
        timestamp: Date.now(),
      }]);
    });

    // For code/debug/test: prepend the plan report (if any) so Claude has the
    // full architectural spec from the planning discussion.
    const effectiveContext = mode !== 'plan' && planReportRef.current
      ? `## 技术方案（来自 Plan 阶段，请严格遵照实施）\n\n${planReportRef.current}\n\n---\n\n${projectContextRef.current ?? ''}`.trimEnd()
      : projectContextRef.current;

    try {
      // plan always creates its own subdirectory — never inherit a pre-existing workspace path
      const invokeWorkspace = mode === 'plan' ? null : wsPath;
      await invoke('run_skill', { mode, task, workspace: invokeWorkspace, context: effectiveContext });

      if (reportContent) {
        // Save the plan report for all subsequent skill invocations
        if (mode === 'plan') planReportRef.current = reportContent;
        setMessages(prev => [...prev, {
          id: makeId(), role: 'director', content: reportContent,
          timestamp: Date.now(), isReport: true,
        }]);
      }
    } catch (err) {
      addMessage('director', `⛔ ${mode} 技能执行失败：${String(err)}`);
      throw err; // propagate so handleSubmit stops the loop
    } finally {
      unlistenChunks();
      unlistenReport();
      unlistenPlanWs();
      unlistenToolLog();
      unlistenBlackboard();
    }

    return wsPath;
  };

  const handleStop = async () => {
    if (!isRunning || isStopping) return;
    setIsStopping(true);
    try { await invoke('cancel_skill'); } catch {}
  };

  const handleOpenProject = async () => {
    const selected = await openDialog({ directory: true, multiple: false, title: '选择项目文件夹' });
    if (!selected) return;
    try {
      const validated = await invoke<string>('open_project', { path: selected as string });
      const meta = projectContextMetaRef.current;
      if (meta.workspace && meta.workspace !== validated) {
        projectContextMetaRef.current = { source: null, workspace: null };
        projectContextRef.current = null;
        setProjectContext(null);
      }
      setWorkspace(validated);
    } catch (err) {
      console.error('open_project error:', err);
    }
  };

  const handleNewChat = useCallback(async () => {
    // Auto-save will have already persisted the current session via the debounced useEffect.
    // Just start fresh with a new session ID.
    setMessages([]);
    setToolLogs([]);
    setBlackboardEvents([]);
    planReportRef.current = '';
    setCurrentSessionId(makeSessionId());
    await invoke('clear_history');
  }, []);

  const handleNewWindow = useCallback(() => {
    invoke('open_new_window').catch(console.error);
  }, []);

  // ── Render ─────────────────────────────────────────────────────────────────

  const modeLabel = currentMode
    ? (MODES.find((m) => m.id === currentMode)?.label ?? currentMode)
    : null;

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
          className="flex items-center justify-between pr-5 pl-24 py-4 flex-shrink-0
                     glass-header relative z-50 select-none"
        >
          <div className="flex items-center gap-2 relative z-50 pointer-events-auto shrink-0">
            {modeLabel && (
              <div className="flex items-center gap-1.5 glass-button px-2 py-1 rounded-lg">
                <span className="text-xs text-zinc-500">skill:</span>
                <span className="text-xs font-medium text-violet-600 dark:text-violet-400">{modeLabel}</span>
              </div>
            )}
          </div>

          {/* Central explicit drag region occupying all empty space */}
          <div data-tauri-drag-region className="flex-1 h-full self-stretch drag-region" style={{ WebkitAppRegion: 'drag' } as React.CSSProperties} />

          <div className="flex items-center gap-3 relative z-50 pointer-events-auto shrink-0">
            <StatusPanel status={status} checking={checking} onRecheck={runDetection} />
            <button
              onClick={() => { setContextDraft(projectContext ?? ''); setShowContextEditor(true); }}
              className={`text-xs transition-colors flex items-center gap-1.5 px-3 py-1.5 rounded-lg glass-button
                          ${projectContext
                  ? 'text-rose-600 dark:text-rose-400'
                  : 'text-zinc-600 dark:text-zinc-300'}`}
              title="项目文档"
            >
              📄
              {projectContext && (
                <span className="hidden sm:inline font-medium">
                  {(projectContext.length / 1024).toFixed(1)}KB
                </span>
              )}
            </button>
            <button
              onClick={handleNewWindow}
              className="text-xs text-zinc-600 dark:text-zinc-300 transition-colors flex items-center gap-1.5 px-3 py-1.5 rounded-lg glass-button"
              title="新建窗口"
            >
              <VscMultipleWindows className="w-3.5 h-3.5" />
              <span className="hidden sm:inline font-medium">新窗口</span>
            </button>
            <div className="glass-button rounded-lg">
              <ThemeToggle />
            </div>
          </div>
        </header>

        {/* Main area: Integrated Activity Bar + Sidebar + Chat */}
        <div className="flex-1 flex min-h-0 bg-transparent overflow-hidden relative">

          {/* Activity Bar (Integrated Minimalist style) */}
          <div className="w-14 h-full flex flex-col items-center py-4 glass-container border-r border-zinc-200/50 dark:border-zinc-800/50 z-30 flex-shrink-0">
            <button
              onClick={() => setActiveSidebarTab(prev => prev === 'explorer' ? null : 'explorer')}
              className={`w-10 h-10 rounded-xl flex justify-center items-center relative transition-all duration-300 cursor-pointer mb-2 ${activeSidebarTab === 'explorer'
                  ? 'text-violet-600 bg-white shadow-sm ring-1 ring-zinc-200/50 dark:bg-zinc-800 dark:text-violet-400 dark:ring-zinc-700/50 shadow-[0_4px_12px_rgba(0,0,0,0.05)]'
                  : 'text-zinc-500 hover:text-zinc-800 hover:bg-zinc-200/50 dark:text-zinc-400 dark:hover:text-zinc-200 dark:hover:bg-zinc-800/50'
                }`}
              title="文件浏览器 (Explorer)"
            >
              <VscFiles className="w-5 h-5 stroke-[0.2]" />
            </button>

            <button
              onClick={() => setActiveSidebarTab(prev => prev === 'logs' ? null : 'logs')}
              className={`w-10 h-10 rounded-xl flex justify-center items-center relative transition-all duration-300 cursor-pointer mb-2 ${activeSidebarTab === 'logs'
                  ? 'text-violet-600 bg-white shadow-sm ring-1 ring-zinc-200/50 dark:bg-zinc-800 dark:text-violet-400 dark:ring-zinc-700/50 shadow-[0_4px_12px_rgba(0,0,0,0.05)]'
                  : 'text-zinc-500 hover:text-zinc-800 hover:bg-zinc-200/50 dark:text-zinc-400 dark:hover:text-zinc-200 dark:hover:bg-zinc-800/50'
                }`}
              title="工具日志 (Tool Logs)"
            >
              <div className="relative">
                <VscTerminal className="w-5 h-5 stroke-[0.2]" />
                {toolLogs.length > 0 && activeSidebarTab !== 'logs' && (
                  <span className="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full bg-blue-500 ring-2 ring-zinc-50/50 dark:ring-zinc-950/50 shadow-[0_0_8px_rgba(59,130,246,0.6)] animate-pulse" />
                )}
              </div>
            </button>

            <button
              onClick={() => setActiveSidebarTab(prev => prev === 'history' ? null : 'history')}
              className={`w-10 h-10 rounded-xl flex justify-center items-center relative transition-all duration-300 cursor-pointer mb-2 ${activeSidebarTab === 'history'
                  ? 'text-violet-600 bg-white shadow-sm ring-1 ring-zinc-200/50 dark:bg-zinc-800 dark:text-violet-400 dark:ring-zinc-700/50 shadow-[0_4px_12px_rgba(0,0,0,0.05)]'
                  : 'text-zinc-500 hover:text-zinc-800 hover:bg-zinc-200/50 dark:text-zinc-400 dark:hover:text-zinc-200 dark:hover:bg-zinc-800/50'
                }`}
              title="历史对话 (History)"
            >
              <div className="relative">
                <VscHistory className="w-5 h-5 stroke-[0.2]" />
                {sessions.length > 0 && activeSidebarTab !== 'history' && (
                  <span className="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full bg-violet-500 ring-2 ring-zinc-50/50 dark:ring-zinc-950/50" />
                )}
              </div>
            </button>

            <button
              onClick={() => setActiveSidebarTab(prev => prev === 'blackboard' ? null : 'blackboard')}
              className={`w-10 h-10 rounded-xl flex justify-center items-center relative transition-all duration-300 cursor-pointer ${activeSidebarTab === 'blackboard'
                  ? 'text-violet-600 bg-white shadow-sm ring-1 ring-zinc-200/50 dark:bg-zinc-800 dark:text-violet-400 dark:ring-zinc-700/50 shadow-[0_4px_12px_rgba(0,0,0,0.05)]'
                  : 'text-zinc-500 hover:text-zinc-800 hover:bg-zinc-200/50 dark:text-zinc-400 dark:hover:text-zinc-200 dark:hover:bg-zinc-800/50'
                }`}
              title="黑板 / Blackboard"
            >
              <div className="relative">
                <VscChecklist className="w-5 h-5 stroke-[0.2]" />
                {blackboardEvents.length > 0 && activeSidebarTab !== 'blackboard' && (
                  <span className="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full bg-blue-500 ring-2 ring-zinc-50/50 dark:ring-zinc-950/50 shadow-[0_0_8px_rgba(59,130,246,0.6)] animate-pulse" />
                )}
              </div>
            </button>
          </div>

          {/* Docked Sidebar Container */}
          <div
            className={`h-full border-r border-zinc-200/50 dark:border-zinc-800/50 glass-container flex-shrink-0 overflow-hidden transition-[width] duration-300 ease-[cubic-bezier(0.16,1,0.3,1)] z-20 ${activeSidebarTab !== null
                ? 'w-[280px] opacity-100'
                : 'w-0 opacity-0 border-none'
              }`}
          >
            <div className="w-[280px] h-full relative">
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
              <div className={`absolute inset-0 transition-opacity duration-300 ${activeSidebarTab === 'blackboard' ? 'opacity-100 z-10 pointer-events-auto' : 'opacity-0 z-0 pointer-events-none'}`}>
                <BlackboardPanel
                  workspacePath={workspace}
                  events={blackboardEvents}
                  onClose={() => setActiveSidebarTab(null)}
                />
              </div>
            </div>
          </div>

          {/* Main Chat Area */}
          <div className="flex-1 flex flex-col relative min-w-0 z-10 basis-0 bg-transparent shadow-[-4px_0_15px_-5px_rgba(0,0,0,0.05)] dark:shadow-[-4px_0_15px_-5px_rgba(0,0,0,0.4)]">
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
                  onSubmit={handleSubmit}
                  onStop={handleStop}
                />
              </div>
            </div>
          </div>
        </div>

      </div>
      {/* Project Context Editor Modal */}
      {showContextEditor && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-zinc-900/40 backdrop-blur-md"
          onClick={() => setShowContextEditor(false)}
        >
          <div
            className="w-full max-w-2xl mx-4 glass-panel border border-white/40 dark:border-zinc-700/50 flex flex-col max-h-[80vh] shadow-[0_16px_40px_rgba(0,0,0,0.1)] dark:shadow-[0_16px_40px_rgba(0,0,0,0.4)] overflow-hidden"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between px-5 py-4 border-b border-zinc-200/50 dark:border-zinc-800/50 bg-white/40 dark:bg-zinc-900/30">
              <div>
                <h2 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200">项目文档 / Project Context</h2>
                <p className="text-xs text-zinc-500 mt-0.5">粘贴开发文档，Claude & Codex 编码时将以此为依据</p>
              </div>
              <button
                onClick={() => setShowContextEditor(false)}
                className="text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-300 text-xl leading-none"
              >×</button>
            </div>
            <textarea
              value={contextDraft}
              onChange={(e) => setContextDraft(e.target.value)}
              placeholder="粘贴你的开发文档、需求说明、技术规范..."
              className="flex-1 p-4 text-sm font-mono text-zinc-800 dark:text-zinc-200 bg-transparent
                         border-none resize-none focus:outline-none min-h-[300px]
                         placeholder-zinc-400 dark:placeholder-zinc-600"
            />
            <div className="flex items-center justify-between px-5 py-3 border-t border-zinc-200/50 dark:border-zinc-800/50 bg-white/40 dark:bg-zinc-900/30">
              <button
                onClick={() => {
                  projectContextMetaRef.current = { source: null, workspace: null };
                  projectContextRef.current = null;
                  setProjectContext(null);
                  setContextDraft('');
                  setShowContextEditor(false);
                }}
                className="text-xs text-rose-500 hover:text-rose-600 transition-colors"
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
