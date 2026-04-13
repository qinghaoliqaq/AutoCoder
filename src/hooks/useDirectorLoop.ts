/**
 * useDirectorLoop — the core workflow orchestrator.
 *
 * Manages the Director ↔ Skill execution loop:
 *   plan → code → review → test → qa → document → END
 * with failure-retry routing (any failure → code → review → ...).
 */

import { useState, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import type { AppMode, ChatMessage, ToolLog, TokenUsage, BlackboardEvent } from '../types';
import { parseInvoke, stripInvoke } from '../invoke';
import {
  buildNextInputAfterReview,
  buildNextInputAfterTestWithEvidence,
  buildNextInputAfterQaWithEvidence,
  buildNextInputAfterDocumentWithEvidence,
  buildNextInputAfterCodeWithEvidence,
} from '../directorFlow';
import { makeId } from '../utils';
import { createSkillRunner } from './useSkillRunner';

let _appWindow: ReturnType<typeof getCurrentWebviewWindow> | null = null;
function getAppWindow() {
  if (!_appWindow) _appWindow = getCurrentWebviewWindow();
  return _appWindow;
}

export interface DirectorLoopDeps {
  workspaceRef: React.MutableRefObject<string | null>;
  projectContextRef: React.MutableRefObject<string | null>;
  projectContextMetaRef: React.MutableRefObject<{ source: 'auto' | 'manual' | null; workspace: string | null }>;
  planReportRef: React.MutableRefObject<string>;
  stopRequestedRef: React.MutableRefObject<boolean>;

  setMessages: React.Dispatch<React.SetStateAction<ChatMessage[]>>;
  setCurrentMode: React.Dispatch<React.SetStateAction<AppMode | null>>;
  setToolLogs: React.Dispatch<React.SetStateAction<ToolLog[]>>;
  setTokenUsages: React.Dispatch<React.SetStateAction<TokenUsage[]>>;
  setBlackboardEvents: React.Dispatch<React.SetStateAction<BlackboardEvent[]>>;
  setWorkspace: React.Dispatch<React.SetStateAction<string | null>>;
}

export function useDirectorLoop(deps: DirectorLoopDeps) {
  const {
    workspaceRef, projectContextRef, projectContextMetaRef, planReportRef, stopRequestedRef,
    setMessages, setCurrentMode, setToolLogs, setTokenUsages, setBlackboardEvents, setWorkspace,
  } = deps;

  const [isRunning, setIsRunning] = useState(false);
  const [isStopping, setIsStopping] = useState(false);

  const lastInvocationRef = useRef<{ skill: AppMode; task: string; wsPath: string | null } | null>(null);

  // ── Message helpers ──────────────────────────────────────────────────────

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
      `**任务已暂停**\n\n**原因**：${reason}\n\n等你确认后，告诉我"已经恢复了"，我会重新执行 ${last ? `\`${last.skill}\`` : '上一个'} 技能。`
    );
  };

  // ── Skill runners ────────────────────────────────────────────────────────

  const { runReview, runTest, runQa, runDocument, runSkill, handleStop } = createSkillRunner({
    workspaceRef, projectContextRef, projectContextMetaRef, planReportRef, stopRequestedRef,
    addMessage, updateMessage,
    setCurrentMode, setToolLogs, setTokenUsages, setBlackboardEvents, setMessages, setWorkspace, setIsStopping,
  });

  // ── Main submit handler (Director loop) ──────────────────────────────────

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
      const MAX_ROUNDS = 30;
      let hitRoundBudget = true;
      let documentFinished = false;

      for (let round = 0; round < MAX_ROUNDS; round++) {
        // ── Ask Director ──────────────────────────────────────────────────
        const replyId = addMessage('director', '', true);
        let accumulated = '';

        const unlisten = await getAppWindow().listen<string>('director-chat-chunk', (event) => {
          accumulated += event.payload;
          updateMessage(replyId, stripInvoke(accumulated), false);
        });

        try {
          await invoke('director_chat', { input: nextInput });
        } catch (err) {
          updateMessage(replyId, `错误：${String(err)}`, false);
          throw err;
        } finally {
          unlisten();
        }

        // ── Check for skill invocation ────────────────────────────────────
        const invocation = parseInvoke(accumulated);
        if (!invocation) {
          if (stopRequestedRef.current) {
            stopRequestedRef.current = false;
            await pauseDirectorLoop('用户手动停止了当前任务。');
          }
          hitRoundBudget = false;
          break;
        }

        if (documentFinished) {
          addMessage(
            'director',
            `**任务已完成**（document 已生成）。忽略 Director 额外的 \`${invocation.skill}\` 调用。`,
          );
          hitRoundBudget = false;
          break;
        }

        setCurrentMode(invocation.skill);
        lastInvocationRef.current = { skill: invocation.skill, task: invocation.task, wsPath: currentWsPath };

        if (invocation.skill === 'review') {
          const reviewResult = await runReview(invocation.task, currentWsPath);
          nextInput = buildNextInputAfterReview(reviewResult);
        } else if (invocation.skill === 'test') {
          const testResult = await runTest(invocation.task, currentWsPath);
          nextInput = await buildNextInputAfterTestWithEvidence(testResult, currentWsPath);
        } else if (invocation.skill === 'qa') {
          const qaResult = await runQa(invocation.task, currentWsPath);
          nextInput = await buildNextInputAfterQaWithEvidence(qaResult, currentWsPath);
        } else if (invocation.skill === 'document') {
          if (!currentWsPath) {
            addMessage('director', 'document 技能需要工作目录。请先运行 plan 技能建立项目目录。');
            hitRoundBudget = false;
            break;
          }
          try {
            await runDocument(invocation.task, currentWsPath);
          } catch {
            hitRoundBudget = false;
            break;
          }
          documentFinished = true;
          nextInput = await buildNextInputAfterDocumentWithEvidence(currentWsPath);
        } else {
          const result = await runSkill(invocation.skill, invocation.task);
          if (result === null) {
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

  return { handleSubmit, isRunning, isStopping, handleStop };
}
