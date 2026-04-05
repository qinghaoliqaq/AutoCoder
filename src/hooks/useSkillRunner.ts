/**
 * Skill execution — runPhase, runReview, runTest, runQa, runSkill, handleStop.
 *
 * Extracted from App.tsx to keep the main component focused on layout
 * and the Director-loop orchestration.
 */

import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import type { AppMode, ChatMessage, ReviewPhaseResult, QaResult, ToolLog, BlackboardEvent } from '../types';
import { makeId } from '../utils';

const appWindow = getCurrentWebviewWindow();

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
  setBlackboardEvents: React.Dispatch<React.SetStateAction<BlackboardEvent[]>>;
  setMessages: React.Dispatch<React.SetStateAction<ChatMessage[]>>;
  setWorkspace: React.Dispatch<React.SetStateAction<string | null>>;
  setIsStopping: React.Dispatch<React.SetStateAction<boolean>>;
  isRunning: boolean;
  isStopping: boolean;
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
  runReview: (task: string, wsPath: string | null) => Promise<{ securityFailed?: boolean; securityIssue?: string }>;
  runTest: (task: string, wsPath: string | null) => Promise<boolean>;
  runQa: (task: string, wsPath: string | null) => Promise<QaResult>;
  runSkill: (mode: AppMode, task: string) => Promise<string | null>;
  handleStop: () => Promise<void>;
}

export function createSkillRunner(deps: SkillRunnerDeps): SkillRunnerActions {
  const {
    workspaceRef, projectContextRef, projectContextMetaRef, planReportRef, stopRequestedRef,
    addMessage, updateMessage,
    setCurrentMode, setToolLogs, setBlackboardEvents, setMessages, setWorkspace, setIsStopping,
    isRunning, isStopping,
  } = deps;

  // ── Shared phase runner ─────────────────────────────────────────────────

  const runPhase = async (
    mode: AppMode,
    phase: string,
    task: string,
    wsPath: string | null,
    issue?: string,
    contextOverride?: string | null,
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

  // ── Review ──────────────────────────────────────────────────────────────

  const runReview = async (
    task: string,
    wsPath: string | null,
  ): Promise<{ securityFailed?: boolean; securityIssue?: string }> => {
    setCurrentMode('review');

    addMessage('director', '🔐 Review 1/2 — Security Audit');
    const sec = await runPhase('review', 'security', task, wsPath);

    addMessage('director', '🧹 Review 2/2 — Code Cleanup');
    await runPhase('review', 'cleanup', task, wsPath);

    setCurrentMode('chat');

    if (!sec.passed) {
      addMessage('director', `⛔ Critical security issue: ${sec.issue}. Security report generated — please fix before testing.`);
      return { securityFailed: true, securityIssue: sec.issue };
    }

    return {};
  };

  // ── Test ────────────────────────────────────────────────────────────────

  const runTest = async (task: string, wsPath: string | null): Promise<boolean> => {
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

    addMessage('director', '📝 Test 1/4 — Generating test plan (Claude + Codex 并行根据 PLAN.md 商讨测试方案...)');
    await phase('gen_test_plan');

    addMessage('director', '🌐 Test 2/4 — Frontend Testing (浏览器自动化测试 UI...)');
    const frontendResult = await phase('frontend_test');
    if (!frontendResult.passed) {
      addMessage('director', `⚠️ 前端测试发现问题：${frontendResult.issue}，已写入 bugs.md，继续后端测试...`);
    }

    addMessage('director', '🧪 Test 3/4 — Integration Testing (启动服务器 + curl 全量接口测试...)');
    let testResult = await phase('integration_test');

    if (!testResult.passed) {
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

    addMessage('director', '📄 Test 4/4 — Generating Project Completion Report...');
    await phase('document');

    setCurrentMode('chat');
    return testResult.passed;
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

    let result: QaResult = {
      verdict: 'FAIL',
      recommended_next_step: 'review',
      summary: 'QA did not return a structured verdict.',
      issue: 'missing qa-result event',
    };
    const unlistenQaResult = await appWindow.listen<QaResult>('qa-result', (event) => {
      result = event.payload;
    });
    const unlistenToolLog = await appWindow.listen<ToolLog>('tool-log', (event) => {
      setToolLogs(prev => [...prev, event.payload]);
    });

    try {
      await invoke('run_skill', {
        mode: 'qa',
        task,
        workspace: wsPath,
        context: qaContext,
        issue: null,
      });
    } finally {
      unlistenChunks();
      unlistenQaResult();
      unlistenToolLog();
      setCurrentMode('chat');
    }

    return result;
  };

  // ── Generic skill (plan, code, debug) ───────────────────────────────────

  const runSkill = async (mode: AppMode, task: string): Promise<string | null> => {
    let wsPath: string | null = workspaceRef.current;
    if (mode !== 'plan' && !wsPath) {
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
      addMessage('director', `⛔ ${mode} 技能执行失败：${String(err)}`);
      throw err;
    } finally {
      unlistenChunks();
      unlistenReport();
      unlistenPlanWs();
      unlistenToolLog();
      unlistenBlackboard();
    }

    return wsPath;
  };

  // ── Stop ────────────────────────────────────────────────────────────────

  const handleStop = async () => {
    if (!isRunning || isStopping) return;
    stopRequestedRef.current = true;
    setIsStopping(true);
    try { await invoke('cancel_skill'); } catch { /* best-effort */ }
  };

  return { runPhase, runReview, runTest, runQa, runSkill, handleStop };
}
