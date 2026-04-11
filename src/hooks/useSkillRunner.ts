/**
 * Skill execution — runPhase, runReview, runTest, runQa, runDocument, runSkill, handleStop.
 *
 * Extracted from App.tsx to keep the main component focused on layout
 * and the Director-loop orchestration.
 */

import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import type { AppMode, ChatMessage, ReviewPhaseResult, QaResult, ToolLog, TokenUsage, BlackboardEvent, SkillError } from '../types';
import { makeId } from '../utils';
import { toast } from 'sonner';

// Lazily initialize to avoid calling Tauri APIs at module import time
// (before the webview runtime is ready), which would crash in tests or SSR.
let _appWindow: ReturnType<typeof getCurrentWebviewWindow> | null = null;
function getAppWindow() {
  if (!_appWindow) _appWindow = getCurrentWebviewWindow();
  return _appWindow;
}

/** Parse a Tauri invoke error into a structured SkillError (best-effort). */
function parseSkillError(err: unknown): SkillError {
  // Tauri v2 serializes the error type directly when E: Serialize
  if (err && typeof err === 'object' && 'kind' in err && 'message' in err) {
    return err as SkillError;
  }
  // Fallback for plain string errors from other commands
  const message = String(err);
  if (message === 'cancelled') {
    return { kind: 'cancelled', message, retryable: false };
  }
  return { kind: 'internal', message, retryable: false };
}

/** Show a toast notification for a skill error (skip cancellation). */
function notifySkillError(error: SkillError, mode: string): void {
  if (error.kind === 'cancelled') return;

  const labels: Record<string, string> = {
    timeout: '执行超时',
    tool_missing: '工具未安装',
    agent_error: 'Agent 错误',
    permission: '权限不足',
    config: '配置错误',
    network: '网络错误',
    api: 'API 错误',
    invalid_mode: '无效模式',
    internal: '内部错误',
  };
  const title = labels[error.kind] ?? '执行失败';
  const description = error.message.length > 200
    ? error.message.slice(0, 200) + '...'
    : error.message;

  if (error.retryable) {
    toast.warning(`${title} (${mode})`, { description, duration: 8000 });
  } else {
    toast.error(`${title} (${mode})`, { description, duration: 8000 });
  }
}

export interface SkillRunnerDeps {
  // Refs (stable across renders)
  workspaceRef: React.MutableRefObject<string | null>;
  projectContextRef: React.MutableRefObject<string | null>;
  projectContextMetaRef: React.MutableRefObject<{ source: 'auto' | 'manual' | null; workspace: string | null }>;
  planReportRef: React.MutableRefObject<string>;
  stopRequestedRef: React.MutableRefObject<boolean>;

  // Callbacks
  addMessage: (role: ChatMessage['role'], content: string, thinking?: boolean) => string;
  updateMessage: (id: string, content: string, thinking?: boolean) => void;

  // State setters
  setCurrentMode: React.Dispatch<React.SetStateAction<AppMode | null>>;
  setToolLogs: React.Dispatch<React.SetStateAction<ToolLog[]>>;
  setTokenUsages: React.Dispatch<React.SetStateAction<TokenUsage[]>>;
  setBlackboardEvents: React.Dispatch<React.SetStateAction<BlackboardEvent[]>>;
  setMessages: React.Dispatch<React.SetStateAction<ChatMessage[]>>;
  setWorkspace: React.Dispatch<React.SetStateAction<string | null>>;
  setIsStopping: React.Dispatch<React.SetStateAction<boolean>>;
}

export interface SkillRunnerActions {
  runPhase: (
    mode: AppMode,
    phase: string,
    task: string,
    wsPath: string | null,
    issue?: string,
    contextOverride?: string | null,
  ) => Promise<ReviewPhaseResult>;
  runReview: (task: string, wsPath: string | null) => Promise<{
    reviewFailed?: boolean;
    reviewIssue?: string;
    securityFailed?: boolean;
    securityIssue?: string;
  }>;
  runTest: (task: string, wsPath: string | null) => Promise<{ passed: boolean; issue: string }>;
  runQa: (task: string, wsPath: string | null) => Promise<QaResult>;
  runDocument: (task: string, wsPath: string | null) => Promise<void>;
  runSkill: (mode: AppMode, task: string) => Promise<string | null>;
  handleStop: () => Promise<void>;
}

export function createSkillRunner(deps: SkillRunnerDeps): SkillRunnerActions {
  const {
    workspaceRef, projectContextRef, projectContextMetaRef, planReportRef, stopRequestedRef,
    addMessage, updateMessage,
    setCurrentMode, setToolLogs, setTokenUsages, setBlackboardEvents, setMessages, setWorkspace, setIsStopping,
  } = deps;

  // ── Shared chunk listener builder ─────────────────────────────────────────
  // Extracted to avoid 3× copy-paste of the same subtask-aware handler.

  function createChunkListener() {
    const agentMsgIds = new Map<string, string>();
    const agentContent = new Map<string, string>();

    const handler = (event: { payload: { agent: string; text: string; reset: boolean; subtask_id?: string } }) => {
      const { agent, text, reset, subtask_id } = event.payload;
      const role = agent as ChatMessage['role'];
      const key = subtask_id ? `${agent}::${subtask_id}` : agent;
      if (reset || !agentMsgIds.has(key)) {
        const id = makeId();
        const msg: ChatMessage = {
          id, role, content: '', timestamp: Date.now(),
          ...(subtask_id ? { subtaskId: subtask_id, subtaskLabel: subtask_id } : {}),
        };
        setMessages(prev => [...prev, msg]);
        agentMsgIds.set(key, id);
        agentContent.set(key, '');
      }
      const id = agentMsgIds.get(key)!;
      const updated = agentContent.get(key)! + text;
      agentContent.set(key, updated);
      updateMessage(id, updated);
    };

    return { handler, agentMsgIds, agentContent };
  }

  // ── Shared phase runner ─────────────────────────────────────────────────

  const runPhase = async (
    mode: AppMode,
    phase: string,
    task: string,
    wsPath: string | null,
    issue?: string,
    contextOverride?: string | null,
  ): Promise<ReviewPhaseResult> => {
    const { handler: chunkHandler } = createChunkListener();
    const unlistenChunks = await getAppWindow().listen('skill-chunk', chunkHandler);

    // Default to FAIL so a missing review-phase-result event can never
    // produce a silent green light. The backend contract is that every
    // phase emits review-phase-result at completion; if it doesn't, treat
    // it as a failure and bubble the issue to the Director.
    let result: ReviewPhaseResult = {
      phase,
      passed: false,
      issue: 'phase did not emit a review-phase-result event',
    };
    const unlistenResult = await getAppWindow().listen<ReviewPhaseResult>('review-phase-result', (event) => {
      result = event.payload;
    });
    const unlistenToolLog = await getAppWindow().listen<ToolLog>('tool-log', (event) => {
      setToolLogs(prev => [...prev, event.payload]);
    });
    const unlistenTokenUsage = await getAppWindow().listen<TokenUsage>('token-usage', (event) => {
      setTokenUsages(prev => [...prev, event.payload]);
    });
    const unlistenBlackboard = await getAppWindow().listen<BlackboardEvent>('blackboard-updated', (event) => {
      setBlackboardEvents(prev => [...prev, event.payload]);
      setMessages(prev => [...prev, {
        id: makeId(),
        role: 'director',
        content: event.payload.summary,
        timestamp: Date.now(),
        ...(event.payload.subtask_id ? { subtaskId: event.payload.subtask_id, subtaskLabel: event.payload.subtask_id } : {}),
      }]);
    });
    const unlistenCompletionReport = await getAppWindow().listen<string>('completion-report', (event) => {
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
      unlistenTokenUsage();
      unlistenBlackboard();
      unlistenCompletionReport();
    }
    return result;
  };

  // ── Review ──────────────────────────────────────────────────────────────

  const runReview = async (
    task: string,
    wsPath: string | null,
  ): Promise<{
    reviewFailed?: boolean;
    reviewIssue?: string;
    securityFailed?: boolean;
    securityIssue?: string;
  }> => {
    setCurrentMode('review');
    const failures: string[] = [];

    addMessage('director', '**Review 1/4** — Plan Check');
    const planResult = await runPhase('review', 'plan_check', task, wsPath);
    if (planResult.passed) {
      addMessage('director', 'Plan Check passed — all planned features verified.');
    } else {
      failures.push(`Plan Check: ${planResult.issue}`);
      addMessage('director', `Plan Check failed: ${planResult.issue}`);
    }

    addMessage('director', '**Review 2/4** — Security Audit');
    const sec = await runPhase('review', 'security', task, wsPath);
    if (sec.passed) {
      addMessage('director', 'Security Audit passed — no critical issues found.');
    } else {
      failures.push(`Security: ${sec.issue}`);
      addMessage('director', `Security Audit failed: ${sec.issue}`);
    }

    addMessage('director', '**Review 3/4** — Specialist Review');
    const specResult = await runPhase('review', 'specialist_review', task, wsPath);
    if (specResult.passed) {
      addMessage('director', 'Specialist Review passed — all specialists approved.');
    } else {
      failures.push(`Specialist: ${specResult.issue}`);
      addMessage('director', `Specialist Review failed: ${specResult.issue}`);
    }

    addMessage('director', '**Review 4/4** — Code Cleanup');
    const cleanResult = await runPhase('review', 'cleanup', task, wsPath);
    if (cleanResult.passed) {
      addMessage('director', 'Code Cleanup passed.');
    } else {
      failures.push(`Cleanup: ${cleanResult.issue}`);
      addMessage('director', `Code Cleanup failed: ${cleanResult.issue}`);
    }

    setCurrentMode('chat');

    // Summary
    if (failures.length > 0) {
      addMessage('director', `**Review complete** — ${failures.length} phase(s) failed:\n${failures.map(f => `- ${f}`).join('\n')}`);
    } else {
      addMessage('director', '**Review complete** — all 4 phases passed.');
    }

    if (failures.length > 0) {
      return {
        reviewFailed: true,
        reviewIssue: failures.join('; '),
        securityFailed: !sec.passed,
        securityIssue: !sec.passed ? sec.issue : undefined,
      };
    }

    return {};
  };

  // ── Test ────────────────────────────────────────────────────────────────

  const runTest = async (
    task: string,
    wsPath: string | null,
  ): Promise<{ passed: boolean; issue: string }> => {
    setCurrentMode('test');

    const contextSections = [
      planReportRef.current
        ? `## 技术方案 / 计划功能（请以此为测试 checklist 基准）\n\n${planReportRef.current}`
        : null,
      projectContextRef.current,
    ].filter((section): section is string => Boolean(section && section.trim()));
    const testContext = contextSections.length > 0
      ? contextSections.join('\n\n---\n\n')
      : null;

    const phase = (p: string, issue?: string) =>
      runPhase('test', p, task, wsPath, issue, testContext);

    addMessage('director', '**Test 1/3** — Generating test plan (Claude + Codex 并行根据 PLAN.md 商讨测试方案...)');
    await phase('gen_test_plan');

    addMessage('director', '**Test 2/3** — Frontend Testing (浏览器自动化测试 UI...)');
    const frontendResult = await phase('frontend_test');
    if (!frontendResult.passed) {
      addMessage('director', `前端测试发现问题：${frontendResult.issue}，已写入 bugs.md，继续后端测试...`);
    }

    addMessage('director', '**Test 3/3** — Integration Testing (启动服务器 + curl 全量接口测试...)');
    let testResult = await phase('integration_test');

    // Single self-heal attempt: let Claude fix what bugs.md lists, then re-run.
    // If that still fails, propagate failure to the Director loop so the
    // Director model can invoke `code` again (the desired flow: test → code).
    if (!testResult.passed) {
      const bugsNote = `bugs.md 已在工作目录中记录所有失败用例，请逐条修复。失败摘要：${testResult.issue}`;
      addMessage('director', '测试失败，已生成 bugs.md。尝试一次 Claude 自愈修复...');
      const fix = await phase('fix', bugsNote);

      if (fix.passed) {
        addMessage('director', 'Claude 完成修复，重新运行集成测试...');
        testResult = await phase('integration_test');
      }

      if (!testResult.passed) {
        addMessage('director', '自愈一轮后仍未通过，将失败信息上报 Director 以调用 code 修复。');
      }
    }

    setCurrentMode('chat');
    return { passed: testResult.passed, issue: testResult.issue };
  };

  // ── QA ──────────────────────────────────────────────────────────────────

  const runQa = async (task: string, wsPath: string | null): Promise<QaResult> => {
    if (!wsPath) {
      throw new Error('没有工作目录。请先运行 plan 技能建立项目目录，再执行 qa。');
    }
    setCurrentMode('qa');
    const qaSections = [
      planReportRef.current
        ? `## 技术方案（来自 Plan 阶段）\n\n${planReportRef.current}`
        : null,
      projectContextRef.current,
    ].filter((section): section is string => Boolean(section && section.trim()));
    const qaContext = qaSections.length > 0
      ? qaSections.join('\n\n---\n\n')
      : null;

    const { handler: chunkHandler } = createChunkListener();
    const unlistenChunks = await getAppWindow().listen('skill-chunk', chunkHandler);

    let result: QaResult = {
      verdict: 'FAIL',
      recommended_next_step: 'code',
      summary: 'QA did not return a structured verdict.',
      issue: 'missing qa-result event',
    };
    const unlistenQaResult = await getAppWindow().listen<QaResult>('qa-result', (event) => {
      result = event.payload;
    });
    const unlistenToolLog = await getAppWindow().listen<ToolLog>('tool-log', (event) => {
      setToolLogs(prev => [...prev, event.payload]);
    });
    const unlistenTokenUsage = await getAppWindow().listen<TokenUsage>('token-usage', (event) => {
      setTokenUsages(prev => [...prev, event.payload]);
    });
    const unlistenBlackboard = await getAppWindow().listen<BlackboardEvent>('blackboard-updated', (event) => {
      setBlackboardEvents(prev => [...prev, event.payload]);
    });

    try {
      await invoke('run_skill', {
        mode: 'qa',
        task,
        workspace: wsPath,
        context: qaContext,
        issue: null,
        phase: null,
      });
    } finally {
      unlistenChunks();
      unlistenQaResult();
      unlistenToolLog();
      unlistenTokenUsage();
      unlistenBlackboard();
      setCurrentMode('chat');
    }

    return result;
  };

  // ── Document ────────────────────────────────────────────────────────────

  const runDocument = async (task: string, wsPath: string | null): Promise<void> => {
    if (!wsPath) {
      addMessage('director', '没有工作目录。请先运行 plan 技能建立项目目录，再执行 document。');
      throw new Error('document skill requires workspace');
    }
    setCurrentMode('document');

    const { handler: chunkHandler } = createChunkListener();
    const unlistenChunks = await getAppWindow().listen('skill-chunk', chunkHandler);
    const unlistenToolLog = await getAppWindow().listen<ToolLog>('tool-log', (event) => {
      setToolLogs(prev => [...prev, event.payload]);
    });
    const unlistenTokenUsage = await getAppWindow().listen<TokenUsage>('token-usage', (event) => {
      setTokenUsages(prev => [...prev, event.payload]);
    });
    const unlistenBlackboard = await getAppWindow().listen<BlackboardEvent>('blackboard-updated', (event) => {
      setBlackboardEvents(prev => [...prev, event.payload]);
    });
    const unlistenCompletionReport = await getAppWindow().listen<string>('completion-report', (event) => {
      setMessages(prev => [...prev, {
        id: makeId(),
        role: 'director',
        content: event.payload,
        timestamp: Date.now(),
        isReport: true,
      }]);
    });

    try {
      await invoke('run_skill', {
        mode: 'document',
        task,
        workspace: wsPath,
        context: projectContextRef.current,
        phase: null,
        issue: null,
      });
    } catch (err) {
      const skillErr = parseSkillError(err);
      if (skillErr.kind !== 'cancelled') {
        addMessage('director', `**document 技能执行失败：**${skillErr.message}`);
        notifySkillError(skillErr, 'document');
      }
      throw err;
    } finally {
      unlistenChunks();
      unlistenToolLog();
      unlistenTokenUsage();
      unlistenBlackboard();
      unlistenCompletionReport();
      setCurrentMode('chat');
    }
  };

  // ── Generic skill (plan, code, debug) ───────────────────────────────────

  const runSkill = async (mode: AppMode, task: string): Promise<string | null> => {
    let wsPath: string | null = workspaceRef.current;
    if (mode !== 'plan' && !wsPath) {
      addMessage('director', '没有工作目录。请先运行 plan 技能建立项目目录，再执行 code / review / test。');
      return null;
    }

    const { handler: chunkHandler } = createChunkListener();
    const unlistenChunks = await getAppWindow().listen('skill-chunk', chunkHandler);

    let reportContent = '';
    const unlistenReport = await getAppWindow().listen<string>('plan-report', (event) => {
      reportContent = event.payload;
    });
    const unlistenPlanWs = await getAppWindow().listen<string>('plan-workspace', (event) => {
      wsPath = event.payload;
      const meta = projectContextMetaRef.current;
      if (meta.source === 'manual' && meta.workspace === null && projectContextRef.current !== null) {
        projectContextMetaRef.current = { ...meta, workspace: event.payload };
      }
      setWorkspace(event.payload);
    });
    const unlistenToolLog = await getAppWindow().listen<ToolLog>('tool-log', (event) => {
      setToolLogs(prev => [...prev, event.payload]);
    });
    const unlistenTokenUsage = await getAppWindow().listen<TokenUsage>('token-usage', (event) => {
      setTokenUsages(prev => [...prev, event.payload]);
    });
    const unlistenBlackboard = await getAppWindow().listen<BlackboardEvent>('blackboard-updated', (event) => {
      setBlackboardEvents(prev => [...prev, event.payload]);
      setMessages(prev => [...prev, {
        id: makeId(),
        role: 'director',
        content: event.payload.summary,
        timestamp: Date.now(),
        ...(event.payload.subtask_id ? { subtaskId: event.payload.subtask_id, subtaskLabel: event.payload.subtask_id } : {}),
      }]);
    });

    const effectiveContext = mode !== 'plan' && planReportRef.current
      ? `## 技术方案（来自 Plan 阶段，请严格遵照实施）\n\n${planReportRef.current}\n\n---\n\n${projectContextRef.current ?? ''}`.trimEnd()
      : projectContextRef.current;

    try {
      const invokeWorkspace = mode === 'plan' ? null : wsPath;
      await invoke('run_skill', { mode, task, workspace: invokeWorkspace, context: effectiveContext });

      if (reportContent) {
        if (mode === 'plan') planReportRef.current = reportContent;
        setMessages(prev => [...prev, {
          id: makeId(), role: 'director', content: reportContent,
          timestamp: Date.now(), isReport: true,
        }]);
      }
    } catch (err) {
      const skillErr = parseSkillError(err);
      if (skillErr.kind !== 'cancelled') {
        addMessage('director', `**${mode} 技能执行失败：**${skillErr.message}`);
        notifySkillError(skillErr, mode);
      }
      throw err;
    } finally {
      unlistenChunks();
      unlistenReport();
      unlistenPlanWs();
      unlistenToolLog();
      unlistenTokenUsage();
      unlistenBlackboard();
    }

    return wsPath;
  };

  // ── Stop ────────────────────────────────────────────────────────────────

  const handleStop = async () => {
    // Use stopRequestedRef (a ref, always current) instead of isRunning /
    // isStopping (closure values, stale after setIsRunning was called but
    // before React re-rendered and recreated this function).
    if (stopRequestedRef.current) return;
    stopRequestedRef.current = true;
    setIsStopping(true);
    try { await invoke('cancel_skill'); } catch { /* best-effort */ }
  };

  return { runPhase, runReview, runTest, runQa, runDocument, runSkill, handleStop };
}
