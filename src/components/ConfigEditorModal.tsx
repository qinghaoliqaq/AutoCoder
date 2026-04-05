import { ConfigDraft, SystemStatus, AGENT_PROVIDERS } from '../types';
import AccessModeToggle from './AccessModeToggle';
import ToggleSwitch from './ToggleSwitch';
import { CheckCircle2, AlertTriangle, LoaderCircle } from 'lucide-react';

interface ConfigEditorModalProps {
  draft: ConfigDraft | null;
  status: SystemStatus | null;
  checking: boolean;
  saving: boolean;
  error: string | null;
  onClose: () => void;
  onChange: (draft: ConfigDraft) => void;
  onSave: () => void;
  onRecheckEnvironment: () => void;
}

export default function ConfigEditorModal({
  draft,
  status,
  checking,
  saving,
  error,
  onClose,
  onChange,
  onSave,
  onRecheckEnvironment,
}: ConfigEditorModalProps) {
  const update = <K extends keyof ConfigDraft>(key: K, value: ConfigDraft[K]) => {
    if (!draft) return;
    onChange({ ...draft, [key]: value });
  };

  const claudeInstalled = status?.claude.installed ?? false;
  const codexInstalled = status?.codex.installed ?? false;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-white/20 backdrop-blur-xl dark:bg-zinc-950/60"
      onClick={onClose}
    >
      <div
        className="mx-4 flex max-h-[88vh] w-full max-w-3xl flex-col overflow-hidden glass-panel"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-4 border-b border-zinc-200/40 bg-white/10 px-5 py-4 dark:border-zinc-800/50 dark:bg-zinc-900/25">
          <div>
            <h2 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200">设置</h2>
            <p className="mt-0.5 text-xs leading-5 text-zinc-500">
              模型配置、执行权限和本地工具状态都集中在这里。保存后会写入应用配置目录中的 <code>config.toml</code>。
            </p>
          </div>
          <button
            onClick={onClose}
            className="text-xl leading-none text-zinc-400 transition-colors hover:text-zinc-600 dark:hover:text-zinc-300"
            title="关闭"
          >
            ×
          </button>
        </div>

        {draft ? (
          <div className="custom-scrollbar grid gap-4 overflow-y-auto px-5 py-4 md:grid-cols-2">
            <div className="md:col-span-2 rounded-2xl border border-white/50 bg-white/30 px-5 py-4 shadow-[0_12px_30px_rgba(15,23,42,0.06)] backdrop-blur-xl dark:border-white/10 dark:bg-zinc-900/40 dark:shadow-[0_12px_30px_rgba(0,0,0,0.24)]">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <div className="text-[11px] font-semibold uppercase tracking-[0.08em] text-zinc-500">Environment</div>
                  <div className="mt-1 text-sm font-semibold text-zinc-800 dark:text-zinc-100">本地工具环境检测</div>
                </div>
                <button
                  type="button"
                  onClick={onRecheckEnvironment}
                  disabled={checking}
                  className="rounded-lg px-3 py-1.5 text-xs font-medium text-zinc-600 glass-button dark:text-zinc-300 disabled:cursor-not-allowed disabled:opacity-60"
                >
                  {checking ? '检测中...' : '重新检测'}
                </button>
              </div>
              <div className="mt-3 grid gap-3 sm:grid-cols-2">
                <div className="rounded-xl border border-zinc-200/60 bg-white/55 px-3.5 py-3 dark:border-zinc-800 dark:bg-zinc-950/45">
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-sm font-medium text-zinc-800 dark:text-zinc-100">Claude Code</span>
                    {checking ? (
                      <LoaderCircle className="h-4 w-4 animate-spin text-amber-500" />
                    ) : claudeInstalled ? (
                      <CheckCircle2 className="h-4 w-4 text-emerald-500" />
                    ) : (
                      <AlertTriangle className="h-4 w-4 text-rose-500" />
                    )}
                  </div>
                  <div className="mt-1.5 text-xs text-zinc-500 dark:text-zinc-400">
                    {claudeInstalled ? '已检测到本地 Claude Code CLI' : '未检测到 Claude Code CLI'}
                  </div>
                </div>
                <div className="rounded-xl border border-zinc-200/60 bg-white/55 px-3.5 py-3 dark:border-zinc-800 dark:bg-zinc-950/45">
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-sm font-medium text-zinc-800 dark:text-zinc-100">OpenAI Codex</span>
                    {checking ? (
                      <LoaderCircle className="h-4 w-4 animate-spin text-amber-500" />
                    ) : codexInstalled ? (
                      <CheckCircle2 className="h-4 w-4 text-emerald-500" />
                    ) : (
                      <AlertTriangle className="h-4 w-4 text-rose-500" />
                    )}
                  </div>
                  <div className="mt-1.5 text-xs text-zinc-500 dark:text-zinc-400">
                    {codexInstalled ? '已检测到本地 Codex CLI' : '未检测到 Codex CLI'}
                  </div>
                </div>
              </div>
            </div>

            <label className="flex flex-col gap-1.5 md:col-span-2">
              <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-zinc-500">API Key</span>
              <input
                type="password"
                value={draft.api_key}
                onChange={(event) => update('api_key', event.target.value)}
                placeholder="输入主导模型 API Key"
                className="rounded-xl border border-zinc-200/80 bg-white/70 px-3 py-2.5 text-sm text-zinc-800 outline-none transition focus:border-violet-300 dark:border-zinc-700 dark:bg-zinc-950/60 dark:text-zinc-100"
              />
            </label>

            <label className="flex flex-col gap-1.5">
              <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-zinc-500">API Format</span>
              <select
                value={draft.api_format}
                onChange={(event) => update('api_format', event.target.value as ConfigDraft['api_format'])}
                className="rounded-xl border border-zinc-200/80 bg-white/70 px-3 py-2.5 text-sm text-zinc-800 outline-none transition focus:border-violet-300 dark:border-zinc-700 dark:bg-zinc-950/60 dark:text-zinc-100"
              >
                <option value="openai">OpenAI Compatible</option>
                <option value="anthropic">Anthropic Compatible</option>
              </select>
            </label>

            <label className="flex flex-col gap-1.5">
              <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-zinc-500">Model</span>
              <input
                type="text"
                value={draft.model}
                onChange={(event) => update('model', event.target.value)}
                placeholder="gpt-4o / claude-sonnet-4-6 / MiniMax-M2.5"
                className="rounded-xl border border-zinc-200/80 bg-white/70 px-3 py-2.5 text-sm text-zinc-800 outline-none transition focus:border-violet-300 dark:border-zinc-700 dark:bg-zinc-950/60 dark:text-zinc-100"
              />
            </label>

            <label className="flex flex-col gap-1.5 md:col-span-2">
              <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-zinc-500">Base URL</span>
              <input
                type="text"
                value={draft.base_url}
                onChange={(event) => update('base_url', event.target.value)}
                placeholder="https://api.openai.com/v1"
                className="rounded-xl border border-zinc-200/80 bg-white/70 px-3 py-2.5 text-sm text-zinc-800 outline-none transition focus:border-violet-300 dark:border-zinc-700 dark:bg-zinc-950/60 dark:text-zinc-100"
              />
            </label>

            <label className="flex flex-col gap-1.5">
              <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-zinc-500">Parallel Lanes</span>
              <input
                type="number"
                min={1}
                max={8}
                value={draft.max_parallel_subtasks}
                onChange={(event) => update('max_parallel_subtasks', Math.max(1, Math.min(8, Number(event.target.value) || 1)))}
                className="rounded-xl border border-zinc-200/80 bg-white/70 px-3 py-2.5 text-sm text-zinc-800 outline-none transition focus:border-violet-300 dark:border-zinc-700 dark:bg-zinc-950/60 dark:text-zinc-100"
              />
            </label>

            <label className="flex flex-col gap-1.5">
              <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-zinc-500">Execution Access</span>
              <AccessModeToggle
                mode={draft.execution_access_mode}
                onChange={(mode) => update('execution_access_mode', mode)}
              />
            </label>

            <div className="inline-flex items-center gap-2 rounded-2xl border border-white/50 bg-white/30 px-5 py-4 shadow-[0_12px_30px_rgba(15,23,42,0.06)] backdrop-blur-xl dark:border-white/10 dark:bg-zinc-900/40 dark:shadow-[0_12px_30px_rgba(0,0,0,0.24)]">
              <div className="min-w-0">
                <div className="font-semibold text-zinc-700 dark:text-zinc-200">
                  Vendored Skills
                </div>
                <div className="mt-0.5 text-[11px] leading-4 text-zinc-500 dark:text-zinc-400">
                  允许系统按子任务自动注入仓库内置的 packaged skills。
                </div>
              </div>

              <div className="ml-auto">
                <ToggleSwitch
                  checked={draft.vendored_skills}
                  onChange={(checked) => update('vendored_skills', checked)}
                  accent="violet"
                  title="切换 vendored skills"
                />
              </div>
            </div>

            {/* ── Agent Provider Section ──────────────────────────────── */}
            <div className="md:col-span-2 rounded-2xl border border-white/50 bg-white/30 px-5 py-4 shadow-[0_12px_30px_rgba(15,23,42,0.06)] backdrop-blur-xl dark:border-white/10 dark:bg-zinc-900/40 dark:shadow-[0_12px_30px_rgba(0,0,0,0.24)]">
              <div className="text-[11px] font-semibold uppercase tracking-[0.08em] text-zinc-500">Agent Provider</div>
              <div className="mt-1 text-sm font-semibold text-zinc-800 dark:text-zinc-100">智能体执行层</div>
              <p className="mt-1 text-[11px] leading-4 text-zinc-500 dark:text-zinc-400">
                配置后，技能（代码、调试、测试等）将通过 API 直接执行，无需安装 CLI。工具（Bash、编辑器、Grep、Glob）在本地 Rust 中运行，完全免费。
              </p>

              <div className="mt-3 grid gap-3 sm:grid-cols-2">
                <label className="flex flex-col gap-1.5">
                  <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-zinc-500">供应商</span>
                  <select
                    value={draft.agent_provider}
                    onChange={(event) => update('agent_provider', event.target.value)}
                    className="rounded-xl border border-zinc-200/80 bg-white/70 px-3 py-2.5 text-sm text-zinc-800 outline-none transition focus:border-violet-300 dark:border-zinc-700 dark:bg-zinc-950/60 dark:text-zinc-100"
                  >
                    {AGENT_PROVIDERS.map((p) => (
                      <option key={p.value} value={p.value}>{p.label}</option>
                    ))}
                  </select>
                </label>

                <label className="flex flex-col gap-1.5">
                  <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-zinc-500">API Key</span>
                  <input
                    type="password"
                    value={draft.agent_api_key}
                    onChange={(event) => update('agent_api_key', event.target.value)}
                    placeholder={draft.agent_provider ? '输入供应商 API Key' : '请先选择供应商'}
                    disabled={!draft.agent_provider}
                    className="rounded-xl border border-zinc-200/80 bg-white/70 px-3 py-2.5 text-sm text-zinc-800 outline-none transition focus:border-violet-300 disabled:opacity-50 dark:border-zinc-700 dark:bg-zinc-950/60 dark:text-zinc-100"
                  />
                </label>

                <label className="flex flex-col gap-1.5">
                  <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-zinc-500">Model <span className="normal-case font-normal text-zinc-400">(留空用默认)</span></span>
                  <input
                    type="text"
                    value={draft.agent_model}
                    onChange={(event) => update('agent_model', event.target.value)}
                    placeholder="使用供应商默认模型"
                    disabled={!draft.agent_provider}
                    className="rounded-xl border border-zinc-200/80 bg-white/70 px-3 py-2.5 text-sm text-zinc-800 outline-none transition focus:border-violet-300 disabled:opacity-50 dark:border-zinc-700 dark:bg-zinc-950/60 dark:text-zinc-100"
                  />
                </label>

                <label className="flex flex-col gap-1.5">
                  <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-zinc-500">Base URL <span className="normal-case font-normal text-zinc-400">(留空用默认)</span></span>
                  <input
                    type="text"
                    value={draft.agent_base_url}
                    onChange={(event) => update('agent_base_url', event.target.value)}
                    placeholder="使用供应商默认端点"
                    disabled={!draft.agent_provider}
                    className="rounded-xl border border-zinc-200/80 bg-white/70 px-3 py-2.5 text-sm text-zinc-800 outline-none transition focus:border-violet-300 disabled:opacity-50 dark:border-zinc-700 dark:bg-zinc-950/60 dark:text-zinc-100"
                  />
                </label>
              </div>

              {draft.agent_provider && !draft.agent_api_key && (
                <div className="mt-3 rounded-xl border border-amber-200/60 bg-amber-50/70 px-3 py-2 text-[11px] leading-4 text-amber-700 dark:border-amber-500/20 dark:bg-amber-500/10 dark:text-amber-300">
                  需要填写 API Key 才能启用 Agent 执行层。填写后技能将通过 API 直接调用，不再依赖本地 CLI。
                </div>
              )}

              {draft.agent_provider && draft.agent_api_key && (
                <div className="mt-3 rounded-xl border border-emerald-200/60 bg-emerald-50/70 px-3 py-2 text-[11px] leading-4 text-emerald-700 dark:border-emerald-500/20 dark:bg-emerald-500/10 dark:text-emerald-300">
                  已配置 ✓ 技能将通过 {AGENT_PROVIDERS.find(p => p.value === draft.agent_provider)?.label ?? draft.agent_provider} API 执行。
                </div>
              )}
            </div>

            <div className="rounded-2xl border border-sky-200/70 bg-sky-50/70 px-4 py-3 text-xs leading-6 text-sky-800 shadow-[0_10px_26px_rgba(56,189,248,0.08)] dark:border-sky-500/20 dark:bg-sky-500/10 dark:text-sky-200">
              Director 的 `openai` 走 `/chat/completions`，`anthropic` 走 `/messages`。Agent 供应商会自动选择正确的协议。
            </div>

            <div className={`text-xs leading-6 md:col-span-2 rounded-2xl px-4 py-3 ${
              draft.execution_access_mode === 'full_access'
                ? 'border border-amber-200/80 bg-amber-50/85 text-amber-800 dark:border-amber-500/20 dark:bg-amber-500/10 dark:text-amber-200'
                : 'border border-emerald-200/70 bg-emerald-50/80 text-emerald-800 dark:border-emerald-500/20 dark:bg-emerald-500/10 dark:text-emerald-200'
            }`}>
              {draft.execution_access_mode === 'full_access'
                ? 'Full Access 会让 Claude / Codex 在写入型子任务中跳过权限限制，自动执行更激进，但风险也更高。'
                : 'Sandbox 会优先使用受限执行权限。审查型任务仍保持只读，不受这里的切换影响。'}
            </div>

            {error && (
              <div className="rounded-2xl border border-rose-200/70 bg-rose-50/80 px-4 py-3 text-xs leading-6 text-rose-700 dark:border-rose-500/20 dark:bg-rose-500/10 dark:text-rose-200 md:col-span-2">
                {error}
              </div>
            )}
          </div>
        ) : error ? (
          <div className="px-5 py-8">
            <div className="rounded-2xl border border-rose-200/70 bg-rose-50/80 px-4 py-3 text-xs leading-6 text-rose-700 dark:border-rose-500/20 dark:bg-rose-500/10 dark:text-rose-200">
              {error}
            </div>
          </div>
        ) : (
          <div className="px-5 py-8 text-sm text-zinc-500 dark:text-zinc-400">正在读取当前配置...</div>
        )}

        <div className="flex items-center justify-between gap-3 border-t border-zinc-200/40 bg-white/10 px-5 py-3 dark:border-zinc-800/50 dark:bg-zinc-900/25">
          <div className="text-[11px] text-zinc-500">
            保存后新对话会直接使用最新配置。
          </div>
          <div className="flex gap-2">
            <button
              onClick={onClose}
              className="rounded-lg px-4 py-1.5 text-xs font-medium text-zinc-600 glass-button dark:text-zinc-300"
            >
              取消
            </button>
            <button
              onClick={onSave}
              disabled={!draft || saving}
              className="rounded-lg bg-violet-600/90 px-4 py-1.5 text-xs font-medium text-white shadow-md shadow-violet-500/20 transition-all hover:bg-violet-600 hover:shadow-lg hover:shadow-violet-500/30 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {saving ? '保存中...' : '保存配置'}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
