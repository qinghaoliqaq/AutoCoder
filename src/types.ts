export type AgentRole = 'claude' | 'codex' | 'director' | 'user';

export type AppMode = 'chat' | 'plan' | 'code' | 'debug' | 'test' | 'review' | 'qa' | 'document';

export interface SystemStatus {
  api_configured: boolean;
  api_provider: string;
  api_model: string;
}

/** Returned by get_config command */
export interface ConfigStatus {
  configured: boolean;
  base_url: string;
  model: string;
  api_format: 'openai' | 'anthropic';
  api_key_hint: string;
  vendored_skills: boolean;
  max_parallel_subtasks: number;
  execution_access_mode: 'sandbox' | 'full_access';
  director_provider: string;
  agent_provider: string;
  agent_second_provider: string;
}

export interface ConfigDraft {
  api_key: string;
  base_url: string;
  model: string;
  /** Director provider name (openai / anthropic / deepseek / ...). */
  director_provider: string;
  vendored_skills: boolean;
  max_parallel_subtasks: number;
  execution_access_mode: 'sandbox' | 'full_access';
  // Agent layer
  agent_provider: string;
  agent_api_key: string;
  agent_base_url: string;
  agent_model: string;
  agent_second_provider: string;
  agent_second_api_key: string;
  agent_second_base_url: string;
  agent_second_model: string;
}

export interface ResolvedProviderInfo {
  provider: string;
  base_url: string;
  model: string;
  api_format: 'openai' | 'anthropic';
}

/** Supported agent providers for the UI dropdown. */
export const AGENT_PROVIDERS: readonly { value: string; label: string; doc_url: string | null }[] = [
  { value: '',            label: '未配置 (Not configured)',     doc_url: null },
  { value: 'anthropic',   label: 'Anthropic Claude',             doc_url: 'https://docs.anthropic.com/zh-CN/docs/models' },
  { value: 'openai',      label: 'OpenAI',                       doc_url: 'https://platform.openai.com/docs/models' },
  { value: 'deepseek',    label: 'DeepSeek',                     doc_url: 'https://platform.deepseek.com/docs/models' },
  { value: 'zhipu',       label: '智谱 GLM',                     doc_url: 'https://open.bigmodel.cn/doc/api#chatglm' },
  { value: 'minimax',     label: 'MiniMax',                      doc_url: 'https://www.minimaxi.com/document/Introduction' },
  { value: 'moonshot',    label: '月之暗面 Moonshot',             doc_url: 'https://platform.moonshot.cn/docs/api/chat' },
  { value: 'qwen',        label: '通义千问 Qwen',                 doc_url: 'https://help.aliyun.com/zh/dashscope/developer-reference' },
  { value: 'yi',          label: '零一万物 Yi',                   doc_url: 'https://www.lingyiwanwu.com/docs' },
  { value: 'baichuan',    label: '百川 Baichuan',                 doc_url: 'https://www.baichuan-ai.com/docs/api' },
  { value: 'groq',        label: 'Groq',                          doc_url: 'https://console.groq.com/docs/models' },
  { value: 'together',    label: 'Together AI',                   doc_url: 'https://docs.together.com/docs/models' },
  { value: 'fireworks',   label: 'Fireworks AI',                  doc_url: 'https://docs.fireworks.ai/models' },
  { value: 'siliconflow', label: '硅基流动 SiliconFlow',           doc_url: 'https://docs.siliconflow.cn' },
] as const;

export interface ChatMessage {
  id: string;
  role: AgentRole;
  content: string;
  timestamp: number;
  thinking?: boolean;
  isReport?: boolean; // plan report document
  /** When set, this message belongs to a specific subtask (parallel code mode). */
  subtaskId?: string;
  /** Human-readable label for the subtask (e.g. "auth-module"). */
  subtaskLabel?: string;
}

export interface ReviewPhaseResult {
  phase: string;
  passed: boolean;
  issue: string;
}

export interface QaResult {
  verdict: 'PASS' | 'PASS_WITH_CONCERNS' | 'FAIL';
  recommended_next_step: 'complete' | 'review' | 'debug' | 'code';
  summary: string;
  issue: string;
  confidence_score?: number;
  health_score?: number;
}

export interface ToolLog {
  agent: 'claude' | 'codex' | 'system';
  tool: string;
  input: string;
  timestamp: number;
}

export interface TokenUsage {
  input_tokens: number;
  output_tokens: number;
  subtask_id?: string;
}

export interface BlackboardEvent {
  subtask_id: string | null;
  status: string;
  summary: string;
}

/** Structured error returned by the `run_skill` Tauri command. */
export interface SkillError {
  kind: 'cancelled' | 'timeout' | 'tool_missing' | 'agent_error' | 'permission'
    | 'config' | 'network' | 'api' | 'invalid_mode' | 'internal';
  message: string;
  retryable: boolean;
}

export interface FileNode {
  name:     string;
  path:     string;
  is_dir:   boolean;
  children: FileNode[];
}

/** Provider that supplied a skill, used for grouping in the UI. */
export type SkillProvider = 'builtin' | 'project' | 'user' | 'claude' | 'codex';

/** Lifecycle event for a user hook. Mirrors `hooks::HookEvent` on the Rust side. */
export type HookEvent = 'pre_tool_use' | 'post_tool_use' | 'stop';

/**
 * One hook entry as stored in `config.toml`. The `matcher` field is
 * required at the type level (Rust serde defaults it to "*" on parse)
 * but ignored for `stop` hooks.
 */
export interface HookEntry {
  matcher: string;
  command: string;
  /** Per-hook timeout in seconds. `null` means use the 30s default. */
  timeout_secs: number | null;
}

/** Returned by `get_hooks_config` and accepted by `save_hooks_config`. */
export interface HooksConfig {
  pre_tool_use: HookEntry[];
  post_tool_use: HookEntry[];
  stop: HookEntry[];
}

/** Returned by the `list_skills` Tauri command. */
export interface SkillSummary {
  name: string;
  label: string;
  description: string;
  category: string | null;
  /** Full markdown body — used to power the inline preview. */
  content: string;
  provider: SkillProvider;
  /** Names of related skills that resolved against the live registry. */
  related: string[];
}

export interface ModeConfig {
  id: AppMode;
  label: string;
  icon: string;
  leader: AgentRole;
  description: string;
  color: string;
  requiresBoth: boolean;
}

export interface SessionMeta {
  id: string;
  title: string;
  workspace_path: string | null;
  created_at: number;
  updated_at: number;
  message_count: number;
}

export interface Session extends SessionMeta {
  messages: ChatMessage[];
  tool_logs: ToolLog[];
  blackboard_events?: BlackboardEvent[];
  project_context?: string | null;
  project_context_source?: 'auto' | 'manual' | null;
  director_history: unknown[];
}

export const MODES: ModeConfig[] = [
  {
    id: 'plan',
    label: 'Plan',
    icon: '◈',
    leader: 'director',
    description: 'Claude & Codex 协作制定技术方案',
    color: 'text-violet-400',
    requiresBoth: true,
  },
  {
    id: 'code',
    label: 'Code',
    icon: '⚡',
    leader: 'claude',
    description: '按子任务编码并由 Codex 即时审查',
    color: 'text-accent-claude',
    requiresBoth: true,
  },
  {
    id: 'debug',
    label: 'Debug',
    icon: '⊙',
    leader: 'codex',
    description: 'Codex 主导问题定位',
    color: 'text-accent-codex',
    requiresBoth: false,
  },
  {
    id: 'test',
    label: 'Test',
    icon: '△',
    leader: 'claude',
    description: 'Claude 主导测试设计',
    color: 'text-emerald-400',
    requiresBoth: false,
  },
  {
    id: 'review',
    label: 'Review',
    icon: '⛊',
    leader: 'claude',
    description: '全局安全审计与代码清理',
    color: 'text-rose-400',
    requiresBoth: false,
  },
  {
    id: 'qa',
    label: 'QA',
    icon: '✓',
    leader: 'claude',
    description: '基于测试证据与 PLAN 验收标准做功能级裁决',
    color: 'text-amber-400',
    requiresBoth: false,
  },
  {
    id: 'document',
    label: 'Document',
    icon: '✎',
    leader: 'claude',
    description: '生成项目完成报告（PROJECT_REPORT.md）',
    color: 'text-lime-400',
    requiresBoth: false,
  },
];
