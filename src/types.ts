export type AgentRole = 'claude' | 'codex' | 'director' | 'user';

export type AppMode = 'chat' | 'plan' | 'code' | 'debug' | 'test' | 'review';

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
}

export interface ChatMessage {
  id: string;
  role: AgentRole;
  content: string;
  timestamp: number;
  thinking?: boolean;
  isReport?: boolean; // plan report document
}

export interface ReviewPhaseResult {
  phase: string;
  passed: boolean;
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
  director_history: unknown[];
}

export const MODES: ModeConfig[] = [
  {
    id: 'plan',
    label: 'Plan',
    icon: '🗺',
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
    icon: '🔍',
    leader: 'codex',
    description: 'Codex 主导问题定位',
    color: 'text-accent-codex',
    requiresBoth: false,
  },
  {
    id: 'test',
    label: 'Test',
    icon: '✅',
    leader: 'claude',
    description: 'Claude 主导测试设计',
    color: 'text-emerald-400',
    requiresBoth: false,
  },
  {
    id: 'review',
    label: 'Review',
    icon: '🔎',
    leader: 'claude',
    description: '全局安全审计与代码清理',
    color: 'text-rose-400',
    requiresBoth: false,
  },
];
