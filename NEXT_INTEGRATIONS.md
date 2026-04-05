# Next Integrations

本文档定义 AI Dev Hub 在当前 `plan -> code -> review -> test -> qa` 架构之上的下一阶段集成方向。

目标不是继续堆更多 mode，而是把现有多智能体闭环做成真正可扩展、可并行、可验收、可恢复的工程系统。

## 当前状态

当前系统已经具备：

- `plan`：生成 `PLAN.md`
- `code`：按子任务实现，并在子任务内联 `Claude -> Codex review -> Claude fix`
- `review`：全局安全审计与清理
- `test`：集成测试与项目报告
- `qa`：里程碑级验收裁决
- shared blackboard：作为子任务协调载体
- vendored skills：可按子任务注入辅助能力

这已经比传统“单 agent 一路写到底”强很多，但还缺 4 个关键层：

- 结构化任务图
- 结构化证据链
- 运行权限分层
- 人类审批点

## 优先级顺序

建议按下面顺序推进：

1. Planner 输出任务图
2. Blackboard 升级为事件流 + 证据流
3. 子任务 verifier
4. Skills orchestration
5. 权限 / sandbox profile
6. Human gate
7. 发布 / 交付层

## 1. Planner 输出任务图

### 为什么先做

现在 `plan` 主要产出自然语言方案和 checklist，足够驱动实现，但还不足够驱动真正稳定的并行编排。

`code` 想要长期支持高质量并行，必须知道：

- 子任务依赖关系
- 哪些任务允许并行
- 每个子任务的验收标准
- 每个子任务需要什么 skill
- 哪些文件或模块预计会被触达

### 建议新增产物

除 `PLAN.md` 外，新增：

- `PLAN_GRAPH.json`
- `PLAN_ACCEPTANCE.json`

### `PLAN_GRAPH.json` 建议结构

```json
{
  "version": 1,
  "project": "智能招聘系统",
  "subtasks": [
    {
      "id": "F1",
      "title": "候选人管理 API",
      "description": "实现候选人 CRUD 与筛选接口",
      "depends_on": [],
      "can_run_in_parallel": true,
      "estimated_scope": "backend",
      "suggested_skill": "fullstack-dev",
      "expected_touch": ["backend/candidates", "api/routes"]
    }
  ]
}
```

### `PLAN_ACCEPTANCE.json` 建议结构

```json
{
  "version": 1,
  "criteria": [
    {
      "subtask_id": "F1",
      "must_have": [
        "提供创建候选人接口",
        "提供列表与筛选接口",
        "非法参数返回 4xx"
      ],
      "evidence_required": [
        "接口测试",
        "代码审查通过"
      ]
    }
  ]
}
```

### 验收标准

- `plan` 除 `PLAN.md` 外稳定输出任务图和验收标准
- `code` 不再依赖纯文本解析 checklist 进行并行调度
- `test` 与 `qa` 可以直接读取结构化验收标准

## 2. Blackboard 升级为事件流 + 证据流

### 当前问题

现在黑板更接近状态板，适合看“进行到哪了”，但不适合回答：

- 为什么这个子任务被判定通过
- review 发现的问题是否真的被修了
- qa 的结论是基于哪些证据做出的

### 建议新增产物

- `BLACKBOARD_EVENTS.jsonl`
- `EVIDENCE_INDEX.json`

### 事件模型建议

每个关键动作都 append-only 记录一条事件：

- `subtask_started`
- `implementation_completed`
- `review_failed`
- `review_passed`
- `fix_completed`
- `test_passed`
- `test_failed`
- `qa_passed`
- `qa_failed`

### 单条事件建议字段

```json
{
  "ts": 1710000000000,
  "subtask_id": "F1",
  "type": "review_failed",
  "agent": "codex",
  "summary": "缺少分页边界处理",
  "artifacts": ["bugs.md", "review-F1-2.md"]
}
```

### 证据索引建议

对每个子任务统一归档：

- 改动文件
- 命令执行记录
- 测试结果
- review findings
- fix 轮次
- qa verdict

### 验收标准

- QA 可以不重新全量扫描项目，而是优先消费结构化证据
- 每个 PASS/FAIL 都能追溯到具体事件和产物

## 3. 子任务 Verifier

### 作用

它不是替代 `review`，而是在子任务内联 review 前后增加一个低成本守门员。

### 建议职责

- 检查改动是否超出子任务预期边界
- 检查是否触达敏感路径
- 检查是否缺少基础测试证据
- 检查是否遗漏 acceptance criteria

### 为什么重要

Codex review 更偏语义质量，Verifier 更偏规则质量。两者叠加后，子任务闭环会稳定很多。

### 建议新增产物

- `VERIFIER.md` 或 `verifier-result.json`

### 验收标准

- 子任务在 merge 前必须同时通过 `verifier + codex review`
- verifier 失败时，Claude 只能修当前子任务，不能直接跳过

## 4. Skills Orchestration

### 当前状态

现在 vendored skills 已经可以按子任务选用，但还不是 planner 驱动的系统级能力。

### 下一步目标

让 `plan` 阶段直接为每个子任务声明建议 skill，再由 `code` 执行器决定是否注入。

### 规则建议

- `frontend` / `screen` / `ui` 子任务：优先 `frontend-dev`
- `api` / `db` / `auth` / `backend` 子任务：优先 `fullstack-dev`
- `qa`：默认不注入实现型 skill
- `review`：默认不注入实现型 skill

### 后续可扩展

未来可以继续加入：

- `security-review`
- `api-design`
- `db-migration`
- `deployment`
- `performance-audit`

### 验收标准

- 每个子任务的 skill 来源可追溯
- skill 注入策略由 planner + orchestrator 控制，而不是运行期临时猜

## 5. Sandbox Profiles

### 目标

不同 mode 使用不同权限模型，避免“语义上说只读，技术上其实还能改”。

### 建议权限分层

- `plan`：只读
- `code`：工作区可写
- `review`：只读
- `qa`：只读，禁 shell
- `test`：允许命令执行，但限制写入测试产物范围

### 实施方向

- Claude runner 增加模式枚举，而不是只靠 prompt 约束
- 对 shell 工具做模式级 allowlist / denylist
- 对 test 输出目录做白名单，例如：
  - `bugs.md`
  - `PROJECT_REPORT.md`
  - `.ai-dev-hub/runtime`

### 验收标准

- 所有 mode 都有清晰权限定义
- 任何越权写入都能 fail closed

## 6. Human Gate

### 为什么需要

如果最终目标是可落地的工程系统，而不是纯 demo，就必须允许人工在关键节点介入。

### 建议加入的 gate

1. `plan` 完成后人工批准再进入 `code`
2. 高风险 `review` 后人工批准是否继续自动修复
3. `qa = PASS_WITH_CONCERNS` 时人工决定是否视为完成
4. 生产发布前人工确认

### UI 建议

在聊天区或黑板区加入：

- `Approve`
- `Reject`
- `Request Rework`
- `Promote to Release`

### 验收标准

- Director 能识别“等待人工决策”状态
- 被 gate 卡住时不会继续自动跑下去

## 7. Release / Delivery Layer

### 目标

让系统的最终输出不只是“代码写完”，而是“可交付”。

### 建议新增产物

- `RELEASE_NOTES.md`
- `DELIVERY_CHECKLIST.md`
- `ARTIFACT_MANIFEST.json`

### 内容建议

- 运行方式
- 环境变量要求
- 已知限制
- 通过的测试范围
- 剩余风险
- 产物路径

### 验收标准

- `qa = PASS` 后可自动生成交付包描述
- 用户不必再人工整理项目完成信息

## 推荐实施顺序

如果只做一项，先做：

`Planner 输出任务图 + 每子任务 acceptance criteria + suggested skill`

如果做两项，顺序是：

1. `PLAN_GRAPH.json + PLAN_ACCEPTANCE.json`
2. `BLACKBOARD_EVENTS.jsonl + EVIDENCE_INDEX.json`

这是因为：

- 没有任务图，就没有稳定并行编排
- 没有证据流，`qa` 很快就会重新退回“读一堆文本做模糊判断”

## 里程碑建议

### M1

- planner 输出结构化任务图
- code 按任务图调度
- skill 按任务图注入

### M2

- blackboard 事件流落盘
- evidence index 建立
- qa 优先基于证据流验收

### M3

- sandbox profiles 完整落地
- verifier 内联到子任务闭环
- human gate 上线

### M4

- release / delivery layer
- 项目级交付包生成

## 结论

`qa` 拆出来以后，最值得继续集成的不是再加一个新 mode，而是把系统升级成：

- planner 产出结构化任务图
- blackboard 保存结构化事件和证据
- code/review/test/qa 全部围绕统一任务图和证据流运转
- 不同 mode 使用真正隔离的权限模型
- 人类在关键节点拥有审批权

这条路线走完后，AI Dev Hub 才会从“多智能体工作流”升级成“可控的多智能体开发系统”。
