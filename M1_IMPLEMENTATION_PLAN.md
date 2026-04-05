# M1 Implementation Plan

本文档把 [NEXT_INTEGRATIONS.md](/Users/liando/Desktop/ai_workspace/ai-dev-hub/NEXT_INTEGRATIONS.md) 里的第一阶段落成可直接实现的任务。

M1 的唯一目标：

让 `plan` 不再只输出 `PLAN.md`，而是同时输出可被 `code / test / qa` 稳定消费的结构化计划文件。

## M1 范围

本阶段只做 3 件事：

1. `plan` 输出 `PLAN_GRAPH.json`
2. `plan` 输出 `PLAN_ACCEPTANCE.json`
3. `code` 优先消费结构化计划，而不是重新从 markdown 猜子任务

不在本阶段处理：

- 事件流黑板
- evidence index
- verifier
- human gate
- release layer

## 一、目标产物

### 1. `PLAN.md`

保留，继续给人看。

### 2. `PLAN_GRAPH.json`

给调度器看。

### 3. `PLAN_ACCEPTANCE.json`

给 `code / test / qa` 看。

## 二、文件格式

## `PLAN_GRAPH.json`

建议第一版 schema：

```json
{
  "version": 1,
  "project_name": "智能招聘系统",
  "project_goal": "一个可管理职位、候选人、面试流程的全栈系统",
  "subtasks": [
    {
      "id": "F1",
      "title": "职位管理 API",
      "description": "实现职位创建、编辑、列表、删除接口",
      "category": "backend",
      "depends_on": [],
      "parallel_group": "backend-core",
      "can_run_in_parallel": true,
      "suggested_skill": "fullstack-dev",
      "expected_touch": [
        "backend/jobs",
        "src-tauri",
        "api"
      ]
    },
    {
      "id": "P1",
      "title": "职位列表页",
      "description": "展示职位列表，支持搜索与状态展示",
      "category": "frontend",
      "depends_on": ["F1"],
      "parallel_group": "ui-main",
      "can_run_in_parallel": false,
      "suggested_skill": "frontend-dev",
      "expected_touch": [
        "src/components",
        "src/pages"
      ]
    }
  ]
}
```

### 字段说明

- `version`: schema 版本，先固定为 `1`
- `project_name`: 项目名
- `project_goal`: 项目目标摘要
- `subtasks`: 子任务列表
- `id`: 子任务唯一 ID，继续沿用 `F1 / P1 / U1` 这种格式
- `category`: 第一版建议只允许 `frontend | backend | fullstack | infra | docs`
- `depends_on`: 依赖子任务 ID 列表
- `parallel_group`: 可选，用于限制并行 lane
- `can_run_in_parallel`: 是否允许并行
- `suggested_skill`: `frontend-dev | fullstack-dev | null`
- `expected_touch`: 预期会触达的目录或模块

## `PLAN_ACCEPTANCE.json`

建议第一版 schema：

```json
{
  "version": 1,
  "project_acceptance": [
    "核心流程可运行",
    "关键页面可访问",
    "关键 API 可验证"
  ],
  "subtasks": [
    {
      "subtask_id": "F1",
      "must_have": [
        "支持职位创建",
        "支持职位列表",
        "非法参数返回 4xx"
      ],
      "must_not": [
        "返回 500 处理普通校验错误"
      ],
      "evidence_required": [
        "代码审查通过",
        "接口测试通过"
      ],
      "qa_focus": [
        "接口字段完整性",
        "错误返回一致性"
      ]
    }
  ]
}
```

### 字段说明

- `project_acceptance`: 项目级必须满足条件
- `must_have`: 子任务必须具备的能力
- `must_not`: 子任务不能出现的行为
- `evidence_required`: 通过该子任务所需证据
- `qa_focus`: QA 重点检查点

## 三、谁来生成

生成方仍然是 `plan`。

第一版不要引入第三个模型，不要新建独立 planner 服务，直接复用现有 `plan` 流程中的 Claude/Codex 讨论结果。

建议流程：

1. Claude + Codex 继续完成现有 planning discussion
2. Claude 负责写人类可读的 `PLAN.md`
3. Claude 再根据最终方案写 `PLAN_GRAPH.json`
4. Claude 再写 `PLAN_ACCEPTANCE.json`
5. Rust 侧在 `plan` 结束前做一次结构校验

## 四、后端改造点

## 1. `src-tauri/src/skills/plan.rs`

需要新增：

- 让 prompt 明确要求输出 3 个文件
- 在 plan 结束前校验 `PLAN_GRAPH.json` 和 `PLAN_ACCEPTANCE.json`
- 如果 JSON 缺失或结构非法，则让 Claude 修正一次

### 建议新增函数

- `read_required_plan_files(workspace: &str) -> Result<...>`
- `validate_plan_graph(json: &str) -> Result<PlanGraph, String>`
- `validate_plan_acceptance(json: &str) -> Result<PlanAcceptance, String>`

## 2. `src-tauri/src/skills/mod.rs`

可以新增共享类型：

- `PlanGraph`
- `PlanSubtask`
- `PlanAcceptance`
- `SubtaskAcceptance`

## 3. 新建 `src-tauri/src/planning_schema.rs`

建议把结构化计划 schema 单独放一个文件，不要散在 `plan.rs` 里。

建议内容：

- serde struct 定义
- 基础校验函数
- enum / allowed values

## 五、前端改造点

第一版前端不需要大改 UI。

只需要保证运行时逻辑能消费结构化计划。

## 1. `src-tauri/src/skills/code.rs`

这是 M1 的核心消费方。

当前 `code` 还是从 `PLAN.md` / blackboard 中抽子任务。M1 后应该改成：

1. 优先读取 `PLAN_GRAPH.json`
2. 如果文件存在且合法，按 graph 生成 blackboard subtasks
3. 如果文件缺失，再 fallback 到旧的 markdown checklist 解析

### 调度规则第一版建议

- 只调度 `depends_on` 全部完成的任务
- `can_run_in_parallel = false` 的任务单独占 lane
- 同一 `parallel_group` 在第一版中最多同时运行 1 个

这能避免一上来就把并行调度做得太复杂。

## 2. `src/App.tsx`

第一版不必感知 graph 细节，但可以在 plan 完成后增加一条简短 director 报告，例如：

- 共识别 `8` 个子任务
- 其中 `3` 个可并行
- 主执行顺序为 `F1 -> F2 -> P1 -> P2`

这一步不是必须，但对可解释性有帮助。

## 六、Prompt 改造点

## 1. `plan` prompt

需要新增规则：

- 除 `PLAN.md` 外，必须写出 `PLAN_GRAPH.json`
- 除 `PLAN.md` 外，必须写出 `PLAN_ACCEPTANCE.json`
- 两个 JSON 必须与 `PLAN.md` 一致
- 不允许出现 JSON 注释
- 所有 `depends_on` 必须引用已存在的子任务 ID
- `suggested_skill` 只能是允许值

## 2. `qa` prompt

在 M1 不要求大改，但可以预埋规则：

- 如果存在 `PLAN_ACCEPTANCE.json`，优先使用它做验收

## 3. `test` prompt

在 M1 可先只加一条：

- 如果存在 `PLAN_ACCEPTANCE.json`，把 `must_have` 和 `evidence_required` 当作 checklist

## 七、兼容策略

为了不打断现有系统，M1 必须支持 fallback。

### fallback 规则

- 有合法 `PLAN_GRAPH.json`：走新逻辑
- 无 `PLAN_GRAPH.json`：走旧 `PLAN.md` checklist 解析
- 有合法 `PLAN_ACCEPTANCE.json`：`test/qa` 使用
- 无 `PLAN_ACCEPTANCE.json`：保持原逻辑

这样即使旧会话、旧项目、旧 workspace 里没有新文件，也不会挂。

## 八、验收标准

M1 完成的标准：

1. `plan` 结束后，workspace 内稳定生成：
   - `PLAN.md`
   - `PLAN_GRAPH.json`
   - `PLAN_ACCEPTANCE.json`
2. JSON 文件能通过 Rust 结构校验
3. `code` 优先读取 `PLAN_GRAPH.json` 调度子任务
4. graph 缺失时旧逻辑仍可工作
5. `cargo test` 与 `npm test` 通过

## 九、推荐实现顺序

建议按下面顺序开发：

1. 定义 Rust schema
2. 给 `plan` 增加 JSON 输出要求
3. 给 `plan` 增加 JSON 校验
4. 改 `code` 优先读取 `PLAN_GRAPH.json`
5. 改 `test/qa` 读取 `PLAN_ACCEPTANCE.json`
6. 补测试

## 十、测试清单

### Rust 单测

- `PLAN_GRAPH.json` 合法样例可解析
- 缺字段时报错
- 非法 `depends_on` 报错
- 非法 `suggested_skill` 报错
- `PLAN_ACCEPTANCE.json` 合法样例可解析

### 集成测试

- 有 graph 文件时，`code` 按 graph 调度
- 无 graph 文件时，`code` fallback 正常
- 有 acceptance 文件时，`qa` 能读到

## 十一、建议文件改动范围

大概率会涉及：

- `src-tauri/src/skills/plan.rs`
- `src-tauri/src/skills/code.rs`
- `src-tauri/src/skills/mod.rs`
- `src-tauri/src/prompts.rs`
- `src-tauri/prompts/...plan...`
- `src-tauri/prompts/qa_claude.md`
- `src-tauri/src/test_skill.rs`
- 新增 `src-tauri/src/planning_schema.rs`

## 十二、结论

M1 不追求一下子把整套系统做满，而是先完成一个最重要的转折：

从“基于 markdown 文本理解任务”

切换到

“基于结构化计划文件驱动任务”

这一步做完后：

- 并行调度会更稳定
- skills 注入会更可控
- `test / qa` 会更容易消费统一标准
- 后续事件流、verifier、human gate 才有坚实基础
