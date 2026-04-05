import type { QaResult } from './types';

export function buildNextInputAfterReview(result: {
  securityFailed?: boolean;
  securityIssue?: string;
}): string {
  if (result.securityFailed) {
    return `Review 安全审查发现严重安全问题，已生成 security.md 报告。\n\n安全问题摘要：${result.securityIssue}\n\n请立即调用 code 技能，按照 security.md 中记录的问题逐一修复，修复后在 security.md 中标记每项问题为已解决。`;
  }

  return 'review 已完成：安全审查 ✓、代码清理 ✓。请立即调用 test 技能对项目进行完整集成测试（启动服务器 + curl 测试所有接口）。';
}

export function buildNextInputAfterTest(): string {
  return 'test 集成测试及项目报告已完成。请立即调用 qa 技能，基于测试结果、黑板状态和项目产物做功能验收，并给出 PASS / PASS_WITH_CONCERNS / FAIL 结论。';
}

export function buildNextInputAfterQa(result: QaResult): string {
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
