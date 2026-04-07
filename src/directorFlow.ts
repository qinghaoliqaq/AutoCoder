import type { QaResult } from './types';
import { invoke } from '@tauri-apps/api/core';

/** Fetch a compact evidence digest from the backend for the given workspace. */
async function getEvidenceDigest(workspace: string | null): Promise<string> {
  if (!workspace) return '';
  try {
    const digest = await invoke<string | null>('evidence_digest', { workspace });
    return digest ? `\n\n${digest}` : '';
  } catch {
    return '';
  }
}

export function buildNextInputAfterReview(result: {
  reviewFailed?: boolean;
  reviewIssue?: string;
  securityFailed?: boolean;
  securityIssue?: string;
}): string {
  if (result.securityFailed) {
    return `Review 安全审查发现严重安全问题，已生成 security.md 报告。\n\n安全问题摘要：${result.securityIssue}\n\n请立即调用 code 技能，按照 security.md 中记录的问题逐一修复，修复后在 security.md 中标记每项问题为已解决。`;
  }

  if (result.reviewFailed) {
    return `Review 未通过。失败摘要：${result.reviewIssue}\n\n请立即调用 code 技能修复这些 review 问题，完成后重新执行 review，再继续 test。`;
  }

  return 'review 已完成：安全审查 ✓、代码清理 ✓。请立即调用 test 技能对项目进行完整集成测试（启动服务器 + curl 测试所有接口）。';
}

function buildNextInputAfterTest(): string {
  return 'test 集成测试及项目报告已完成。请立即调用 qa 技能，基于测试结果、黑板状态和项目产物做功能验收，并给出 PASS / PASS_WITH_CONCERNS / FAIL 结论。';
}

function buildNextInputAfterQa(result: QaResult): string {
  const qaIssue = result.issue === 'none' ? '无' : result.issue;

  if (result.verdict === 'PASS') {
    return `qa 验收通过。摘要：${result.summary}。请用一句话总结结果并结束任务。`;
  }

  if (result.verdict === 'PASS_WITH_CONCERNS') {
    if (result.recommended_next_step === 'review') {
      return `qa 验收结果：PASS_WITH_CONCERNS。摘要：${result.summary}。关注问题：${qaIssue}。请立即调用 review 技能做额外审查，然后继续 test -> qa 复验。`;
    }

    return `qa 验收结果：PASS_WITH_CONCERNS。摘要：${result.summary}。关注问题：${qaIssue}。请用一句话总结当前可用性与剩余风险并结束任务。`;
  }

  if (result.recommended_next_step === 'code') {
    return `qa 验收失败。摘要：${result.summary}。阻塞问题：${qaIssue}。请立即调用 code 技能补齐缺失实现，完成后继续 review -> test -> qa。`;
  }

  if (result.recommended_next_step === 'review') {
    return `qa 验收失败。摘要：${result.summary}。问题：${qaIssue}。请立即调用 review 技能做进一步审查，完成后继续 test -> qa。`;
  }

  return `qa 验收失败。摘要：${result.summary}。阻塞问题：${qaIssue}。请立即调用 debug 技能定位根因并修复，完成后继续 review -> test -> qa。`;
}

/**
 * Enhanced versions that inject evidence digest into the director message.
 * This gives the Director LLM historical context about what happened across
 * the entire skill chain, enabling smarter routing decisions.
 */
export async function buildNextInputAfterQaWithEvidence(
  result: QaResult,
  workspace: string | null,
): Promise<string> {
  const base = buildNextInputAfterQa(result);
  const digest = await getEvidenceDigest(workspace);
  return base + digest;
}

export async function buildNextInputAfterTestWithEvidence(
  workspace: string | null,
): Promise<string> {
  const base = buildNextInputAfterTest();
  const digest = await getEvidenceDigest(workspace);
  return base + digest;
}

export async function buildNextInputAfterCodeWithEvidence(
  skill: string,
  workspace: string | null,
): Promise<string> {
  let base: string;
  if (skill === 'plan') {
    base = 'plan 技能已完成：Claude 完成了 5 轮规划讨论，并将完整架构文档（PLAN.md）写入了项目目录。请用一句话简要说明最终技术方案，然后立即调用 code 技能按照 PLAN.md 开始开发。';
  } else {
    base = `${skill} 技能已完成。code 模式中的功能级 review 已按子任务执行完毕。请立即调用 review 进行最终安全审查和代码清理。`;
  }
  const digest = await getEvidenceDigest(workspace);
  return base + digest;
}
