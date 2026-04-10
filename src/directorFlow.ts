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
    return `Review 安全审查发现严重安全问题，已生成 security.md 报告。\n\n安全问题摘要：${result.securityIssue}\n\n请立即调用 code 技能，按照 security.md 中记录的问题逐一修复，修复后在 security.md 中标记每项问题为已解决。修复完成后流程会重新进入 review。`;
  }

  if (result.reviewFailed) {
    return `Review 未通过。失败摘要：${result.reviewIssue}\n\n请立即调用 code 技能修复这些 review 问题，修复完成后流程会重新进入 review。`;
  }

  return 'review 已完成：安全审查 ✓、代码清理 ✓。请立即调用 test 技能对项目进行完整集成测试（启动服务器 + curl 测试所有接口）。';
}

function buildNextInputAfterTest(result: { passed: boolean; issue: string }): string {
  if (!result.passed) {
    const issue = result.issue && result.issue.trim().length > 0 ? result.issue : '详见 bugs.md';
    return `test 集成测试未通过。bugs.md 已在项目目录中记录所有失败用例。失败摘要：${issue}\n\n请立即调用 code 技能按照 bugs.md 逐条修复（不要调用 debug / review / test，直接 code）。修复完成后流程会重新进入 review → test。`;
  }
  return 'test 集成测试通过。请立即调用 document 技能生成项目完成文档（PROJECT_REPORT.md），包含已实现功能清单、API 端点、启动指南和访问方式。';
}

function buildNextInputAfterDocument(): string {
  return 'document 技能已完成，PROJECT_REPORT.md 已写入项目目录。请用一句话总结项目产物和访问方式（含启动命令 / 访问 URL），然后结束任务。不要再调用任何技能。';
}

/**
 * Enhanced versions that inject evidence digest into the director message.
 * This gives the Director LLM historical context about what happened across
 * the entire skill chain, enabling smarter routing decisions.
 */
export async function buildNextInputAfterTestWithEvidence(
  result: { passed: boolean; issue: string },
  workspace: string | null,
): Promise<string> {
  const base = buildNextInputAfterTest(result);
  const digest = await getEvidenceDigest(workspace);
  return base + digest;
}

export async function buildNextInputAfterDocumentWithEvidence(
  workspace: string | null,
): Promise<string> {
  const base = buildNextInputAfterDocument();
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
