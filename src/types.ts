export type AgentRole = 'claude' | 'codex' | 'director' | 'user';

export type AppMode = 'chat' | 'plan' | 'code' | 'debug' | 'test' | 'review' | 'qa';

export interface ToolInfo {
  installed: boolean;
  version: string | null;
  path: string | null;
}

export interface SystemStatus {
  claude: ToolInfo;
  codex: ToolInfo;
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
}

export interface ConfigDraft {
  api_key: string;
  base_url: string;
  model: string;
  api_format: 'openai' | 'anthropic';
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

/** Supported agent providers for the UI dropdown. */
export const AGENT_PROVIDERS = [
  { value: '',           label: '未配置 (Not configured)' },
  { value: 'anthropic',  label: 'Anthropic Claude' },
  { value: 'openai',     label: 'OpenAI' },
  { value: 'deepseek',   label: 'DeepSeek' },
  { value: 'zhipu',      label: '智谱 GLM' },
  { value: 'minimax',    label: 'MiniMax' },
  { value: 'moonshot',   label: '月之暗面 Moonshot' },
  { value: 'qwen',       label: '通义千问 Qwen' },
  { value: 'yi',         label: '零一万物 Yi' },
  { value: 'baichuan',   label: '百川 Baichuan' },
  { value: 'groq',       label: 'Groq' },
  { value: 'together',   label: 'Together AI' },
  { value: 'fireworks',  label: 'Fireworks AI' },
  { value: 'siliconflow',label: '硅基流动 SiliconFlow' },
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
}

export interface ToolLog {
  agent: 'claude' | 'codex';
  tool: string;
  input: string;
  timestamp: number;
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
    description: '基于测试证据做功能级验收裁决',
    color: 'text-amber-400',
    requiresBoth: false,
  },
];
