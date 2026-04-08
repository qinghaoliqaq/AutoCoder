import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ConfigDraft, AGENT_PROVIDERS } from '../types';
import AccessModeToggle from './AccessModeToggle';
import ToggleSwitch from './ToggleSwitch';
import ProviderSelect from './ProviderSelect';
import { useTheme, THEMES } from './ThemeProvider';
import { CheckCircle2, AlertTriangle, LoaderCircle, Settings2, Bot, Keyboard, Zap, Palette } from 'lucide-react';

// ── Settings tab definitions ─────────────────────────────────────────────────

type SettingsTab = 'general' | 'agent' | 'appearance' | 'shortcuts';

const TABS: { id: SettingsTab; label: string; icon: React.ReactNode }[] = [
  { id: 'general', label: '通用', icon: <Settings2 className="h-4 w-4" /> },
  { id: 'agent', label: '智能体', icon: <Bot className="h-4 w-4" /> },
  { id: 'appearance', label: '外观', icon: <Palette className="h-4 w-4" /> },
  { id: 'shortcuts', label: '快捷键', icon: <Keyboard className="h-4 w-4" /> },
];

// ── Props ────────────────────────────────────────────────────────────────────

interface ConfigEditorModalProps {
  draft: ConfigDraft | null;
  saving: boolean;
  error: string | null;
  onClose: () => void;
  onChange: (draft: ConfigDraft) => void;
  onSave: () => void;
}

// ── Reusable field wrapper ───────────────────────────────────────────────────

function FieldGroup({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label className="flex flex-col gap-1.5">
      <div className="flex items-baseline gap-2">
        <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-zinc-500">{label}</span>
        {hint && <span className="text-[10px] font-normal normal-case text-zinc-400">{hint}</span>}
      </div>
      {children}
    </label>
  );
}

const inputClass =
  'rounded-xl border border-zinc-200/80 bg-white/70 px-3 py-2.5 text-sm text-zinc-800 outline-none transition-colors focus:border-violet-400 focus:ring-2 focus:ring-violet-500/10 dark:border-zinc-700 dark:bg-zinc-950/60 dark:text-zinc-100 dark:focus:border-violet-500';

// ── Main component ───────────────────────────────────────────────────────────

export default function ConfigEditorModal({
  draft,
  saving,
  error,
  onClose,
  onChange,
  onSave,
}: ConfigEditorModalProps) {
  const [activeTab, setActiveTab] = useState<SettingsTab>('general');

  const update = <K extends keyof ConfigDraft>(key: K, value: ConfigDraft[K]) => {
    if (!draft) return;
    onChange({ ...draft, [key]: value });
  };

  // Keyboard: Esc to close, Cmd/Ctrl+1-3 to switch tabs
  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      if (!saving) onClose();
      return;
    }
    if ((e.metaKey || e.ctrlKey) && e.key >= '1' && e.key <= '4') {
      e.preventDefault();
      const idx = parseInt(e.key) - 1;
      if (idx < TABS.length) setActiveTab(TABS[idx].id);
    }
  }, [saving, onClose]);

  useEffect(() => {
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [handleKeyDown]);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-white/20 backdrop-blur-xl dark:bg-zinc-950/60"
      onClick={onClose}
    >
      <div
        className="mx-4 flex w-full max-w-3xl flex-col overflow-hidden glass-panel"
        style={{ height: 'min(580px, 88vh)' }}
        onClick={(event) => event.stopPropagation()}
      >
        {/* ── Header ──────────────────────────────────────────────── */}
        <div className="flex items-start justify-between gap-4 border-b border-zinc-200/40 bg-white/10 px-5 py-4 dark:border-zinc-800/50 dark:bg-zinc-900/25">
          <div>
            <h2 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200">Settings</h2>
            <p className="mt-0.5 text-xs leading-5 text-zinc-500">
              模型配置与执行权限。保存后写入 <code className="text-[10px] rounded bg-zinc-100 px-1 py-0.5 dark:bg-zinc-800">config.toml</code>
            </p>
          </div>
          <button
            onClick={onClose}
            className="rounded-lg p-1 text-zinc-400 transition-colors hover:bg-zinc-100 hover:text-zinc-600 dark:hover:bg-zinc-800 dark:hover:text-zinc-300"
            title="关闭 (Esc)"
          >
            <svg className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* ── Body: sidebar tabs + content ─────────────────────────── */}
        <div className="flex flex-1 min-h-0">
          {/* Tab sidebar */}
          <nav className="w-40 flex-shrink-0 border-r border-zinc-200/40 bg-white/5 py-2 dark:border-zinc-800/50 dark:bg-zinc-900/15">
            {TABS.map(({ id, label, icon }, idx) => (
              <button
                key={id}
                onClick={() => setActiveTab(id)}
                className={`group flex w-full items-center gap-2.5 px-4 py-2 text-left text-[12px] font-medium transition-colors ${
                  activeTab === id
                    ? 'bg-violet-50/70 text-violet-700 dark:bg-violet-500/15 dark:text-violet-300'
                    : 'text-zinc-500 hover:bg-zinc-100/50 hover:text-zinc-700 dark:text-zinc-400 dark:hover:bg-zinc-800/40 dark:hover:text-zinc-200'
                }`}
              >
                {icon}
                <span className="flex-1">{label}</span>
                <kbd className="hidden group-hover:inline-block rounded border border-zinc-200/60 bg-zinc-100/60 px-1 py-0.5 font-mono text-[9px] text-zinc-400 dark:border-zinc-700 dark:bg-zinc-800">
                  {'\u2318'}{idx + 1}
                </kbd>
              </button>
            ))}
          </nav>

          {/* Content area */}
          <div className="flex-1 custom-scrollbar overflow-y-auto">
            {draft ? (
              <div className="p-5">
                {activeTab === 'general' && <GeneralTab draft={draft} update={update} />}
                {activeTab === 'agent' && <AgentTab draft={draft} update={update} />}
                {activeTab === 'appearance' && <AppearanceTab />}
                {activeTab === 'shortcuts' && <ShortcutsTab />}

                {error && (
                  <div className="mt-4 rounded-xl border border-rose-200/70 bg-rose-50/80 px-4 py-3 text-xs leading-5 text-rose-700 dark:border-rose-500/20 dark:bg-rose-500/10 dark:text-rose-200">
                    {error}
                  </div>
                )}
              </div>
            ) : error ? (
              <div className="p-5">
                <div className="rounded-xl border border-rose-200/70 bg-rose-50/80 px-4 py-3 text-xs leading-5 text-rose-700 dark:border-rose-500/20 dark:bg-rose-500/10 dark:text-rose-200">
                  {error}
                </div>
              </div>
            ) : (
              <div className="flex items-center justify-center p-12">
                <LoaderCircle className="h-5 w-5 animate-spin text-zinc-400" />
                <span className="ml-2 text-sm text-zinc-500">正在读取配置...</span>
              </div>
            )}
          </div>
        </div>

        {/* ── Footer ──────────────────────────────────────────────── */}
        <div className="flex items-center justify-between gap-3 border-t border-zinc-200/40 bg-white/10 px-5 py-3 dark:border-zinc-800/50 dark:bg-zinc-900/25">
          <div className="text-[11px] text-zinc-400">
            保存后新对话将使用最新配置
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

// ── Tab: General ─────────────────────────────────────────────────────────────

function GeneralTab({
  draft,
  update,
}: {
  draft: ConfigDraft;
  update: <K extends keyof ConfigDraft>(key: K, value: ConfigDraft[K]) => void;
}) {
  const [testStatus, setTestStatus] = useState<'idle' | 'testing' | 'success' | 'error'>('idle');
  const [testMessage, setTestMessage] = useState('');

  const handleTestConnection = async () => {
    if (!draft.api_key || !draft.base_url || !draft.model) return;
    setTestStatus('testing');
    setTestMessage('');
    try {
      const result = await invoke<string>('test_api_connection', {
        apiKey: draft.api_key,
        baseUrl: draft.base_url,
        model: draft.model,
        apiFormat: draft.api_format,
      });
      setTestStatus('success');
      setTestMessage(result);
    } catch (err) {
      setTestStatus('error');
      setTestMessage(String(err));
    }
  };

  return (
    <div className="space-y-5">
      <SectionHeading title="Director 模型" description="配置对话指挥层（Director）使用的模型和接口" />

      <div className="grid gap-4 sm:grid-cols-2">
        <div className="sm:col-span-2">
          <FieldGroup label="API Key">
            <input
              type="password"
              value={draft.api_key}
              onChange={(e) => update('api_key', e.target.value)}
              placeholder="输入主导模型 API Key"
              className={inputClass}
            />
          </FieldGroup>
        </div>

        <FieldGroup label="API Format">
          <select
            value={draft.api_format}
            onChange={(e) => update('api_format', e.target.value as ConfigDraft['api_format'])}
            className={inputClass}
          >
            <option value="openai">OpenAI Compatible</option>
            <option value="anthropic">Anthropic Compatible</option>
          </select>
        </FieldGroup>

        <FieldGroup label="Model">
          <input
            type="text"
            value={draft.model}
            onChange={(e) => update('model', e.target.value)}
            placeholder="gpt-4o / claude-sonnet-4-6 / MiniMax-M2.5"
            className={inputClass}
          />
        </FieldGroup>

        <div className="sm:col-span-2">
          <FieldGroup label="Base URL">
            <input
              type="text"
              value={draft.base_url}
              onChange={(e) => update('base_url', e.target.value)}
              placeholder="https://api.openai.com/v1"
              className={inputClass}
            />
          </FieldGroup>
        </div>
      </div>

      {/* Connectivity Test */}
      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={handleTestConnection}
          disabled={testStatus === 'testing' || !draft.api_key || !draft.base_url || !draft.model}
          className="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium text-violet-600 glass-button dark:text-violet-400 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {testStatus === 'testing' ? (
            <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <Zap className="h-3.5 w-3.5" />
          )}
          {testStatus === 'testing' ? '测试中...' : '测试连通性'}
        </button>

        {testStatus === 'success' && (
          <span className="flex items-center gap-1 text-[11px] font-medium text-emerald-600 dark:text-emerald-400">
            <CheckCircle2 className="h-3.5 w-3.5" /> {testMessage}
          </span>
        )}
        {testStatus === 'error' && (
          <span className="flex items-center gap-1 text-[11px] font-medium text-rose-600 dark:text-rose-400">
            <AlertTriangle className="h-3.5 w-3.5" /> {testMessage}
          </span>
        )}
      </div>

      <div className="h-px bg-zinc-200/50 dark:bg-zinc-800/50" />

      <SectionHeading title="执行选项" description="控制并行度、权限和内置技能" />

      <div className="grid gap-4 sm:grid-cols-2">
        <FieldGroup label="Parallel Lanes" hint="同时执行的子任务数">
          <input
            type="number"
            min={1}
            max={8}
            value={draft.max_parallel_subtasks}
            onChange={(e) => update('max_parallel_subtasks', Math.max(1, Math.min(8, Number(e.target.value) || 1)))}
            className={inputClass}
          />
        </FieldGroup>

        <FieldGroup label="Execution Access">
          <AccessModeToggle
            mode={draft.execution_access_mode}
            onChange={(mode) => update('execution_access_mode', mode)}
          />
        </FieldGroup>
      </div>

      {/* Vendored Skills */}
      <div className="flex items-center justify-between rounded-xl border border-zinc-200/60 bg-white/40 px-4 py-3 dark:border-zinc-800/60 dark:bg-zinc-900/30">
        <div>
          <div className="text-sm font-medium text-zinc-700 dark:text-zinc-200">Vendored Skills</div>
          <div className="mt-0.5 text-[11px] leading-4 text-zinc-500 dark:text-zinc-400">
            自动为子任务注入仓库内置的 packaged skills
          </div>
        </div>
        <ToggleSwitch
          checked={draft.vendored_skills}
          onChange={(checked) => update('vendored_skills', checked)}
          accent="violet"
          title="切换 vendored skills"
        />
      </div>

      <InfoBanner variant={draft.execution_access_mode === 'full_access' ? 'warning' : 'success'}>
        {draft.execution_access_mode === 'full_access'
          ? 'Full Access 模式下 Claude / Codex 会跳过权限限制，自动执行更激进，但风险也更高。'
          : 'Sandbox 模式下优先使用受限权限。审查型任务保持只读，不受此设置影响。'}
      </InfoBanner>
    </div>
  );
}

// ── Tab: Agent ───────────────────────────────────────────────────────────────

function AgentTab({
  draft,
  update,
}: {
  draft: ConfigDraft;
  update: <K extends keyof ConfigDraft>(key: K, value: ConfigDraft[K]) => void;
}) {
  const providerLabel = AGENT_PROVIDERS.find(p => p.value === draft.agent_provider)?.label ?? draft.agent_provider;
  const [testStatus, setTestStatus] = useState<'idle' | 'testing' | 'success' | 'error'>('idle');
  const [testMessage, setTestMessage] = useState('');

  const handleTestConnection = async () => {
    if (!draft.agent_provider || !draft.agent_api_key) return;
    setTestStatus('testing');
    setTestMessage('');

    // Determine the actual base_url and model based on provider defaults
    const model = draft.agent_model || 'claude-sonnet-4-6';
    const baseUrl = draft.agent_base_url || 'https://api.anthropic.com/v1';
    const format = draft.agent_provider === 'anthropic' ? 'anthropic' : 'openai';

    try {
      const result = await invoke<string>('test_api_connection', {
        apiKey: draft.agent_api_key,
        baseUrl,
        model,
        apiFormat: format,
      });
      setTestStatus('success');
      setTestMessage(result);
    } catch (err) {
      setTestStatus('error');
      setTestMessage(String(err));
    }
  };

  return (
    <div className="space-y-5">
      <SectionHeading
        title="智能体执行层"
        description="配置后，技能（代码、调试、测试等）将通过 API 直接执行。本地工具（Bash、编辑器等）完全免费运行。"
      />

      <div className="grid gap-4 sm:grid-cols-2">
        <FieldGroup label="供应商">
          <ProviderSelect
            value={draft.agent_provider}
            onChange={(value) => update('agent_provider', value)}
          />
        </FieldGroup>

        <FieldGroup label="API Key">
          <input
            type="password"
            value={draft.agent_api_key}
            onChange={(e) => update('agent_api_key', e.target.value)}
            placeholder={draft.agent_provider ? '输入供应商 API Key' : '请先选择供应商'}
            disabled={!draft.agent_provider}
            className={`${inputClass} disabled:opacity-50 disabled:cursor-not-allowed`}
          />
        </FieldGroup>

        <FieldGroup label="Model" hint="执行身份（Claude）">
          <input
            type="text"
            value={draft.agent_model}
            onChange={(e) => update('agent_model', e.target.value)}
            placeholder="使用供应商默认模型"
            disabled={!draft.agent_provider}
            className={`${inputClass} disabled:opacity-50 disabled:cursor-not-allowed`}
          />
        </FieldGroup>

        <FieldGroup label="Review Model" hint="审阅身份（Codex），留空同上">
          <input
            type="text"
            value={draft.agent_review_model}
            onChange={(e) => update('agent_review_model', e.target.value)}
            placeholder="留空则使用执行身份模型"
            disabled={!draft.agent_provider}
            className={`${inputClass} disabled:opacity-50 disabled:cursor-not-allowed`}
          />
        </FieldGroup>

        <div className="sm:col-span-2">
          <FieldGroup label="Base URL" hint="留空用默认">
            <input
              type="text"
              value={draft.agent_base_url}
              onChange={(e) => update('agent_base_url', e.target.value)}
              placeholder="使用供应商默认端点"
              disabled={!draft.agent_provider}
              className={`${inputClass} disabled:opacity-50 disabled:cursor-not-allowed`}
            />
          </FieldGroup>
        </div>
      </div>

      {/* Connectivity Test */}
      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={handleTestConnection}
          disabled={testStatus === 'testing' || !draft.agent_provider || !draft.agent_api_key}
          className="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium text-violet-600 glass-button dark:text-violet-400 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {testStatus === 'testing' ? (
            <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <Zap className="h-3.5 w-3.5" />
          )}
          {testStatus === 'testing' ? '测试中...' : '测试连通性'}
        </button>

        {testStatus === 'success' && (
          <span className="flex items-center gap-1 text-[11px] font-medium text-emerald-600 dark:text-emerald-400">
            <CheckCircle2 className="h-3.5 w-3.5" /> {testMessage}
          </span>
        )}
        {testStatus === 'error' && (
          <span className="flex items-center gap-1 text-[11px] font-medium text-rose-600 dark:text-rose-400">
            <AlertTriangle className="h-3.5 w-3.5" /> {testMessage}
          </span>
        )}
      </div>

      {/* Status banners */}
      {draft.agent_provider && !draft.agent_api_key && (
        <InfoBanner variant="warning">
          需要填写 API Key 才能启用 Agent 执行层。
        </InfoBanner>
      )}

      {draft.agent_provider && draft.agent_api_key && testStatus === 'idle' && (
        <InfoBanner variant="success">
          已配置 — 技能将通过 {providerLabel} API 执行。点击「测试连通性」验证配置是否正确。
        </InfoBanner>
      )}

      {!draft.agent_provider && (
        <InfoBanner variant="info">
          选择供应商并填写 API Key 后即可启用智能体执行。
        </InfoBanner>
      )}
    </div>
  );
}

// ── Tab: Appearance ─────────────────────────────────────────────────────────

function AppearanceTab() {
  const { themePreference, setTheme } = useTheme();
  const lightThemes = THEMES.filter(t => t.mode === 'light');
  const darkThemes = THEMES.filter(t => t.mode === 'dark');

  return (
    <div className="space-y-5">
      <SectionHeading title="主题" description="选择一个颜色主题，或跟随系统偏好自动切换" />

      {/* System toggle */}
      <div className="flex items-center justify-between rounded-xl border border-zinc-200/60 bg-white/40 px-4 py-3 dark:border-zinc-800/60 dark:bg-zinc-900/30">
        <div>
          <div className="text-sm font-medium text-zinc-700 dark:text-zinc-200">跟随系统</div>
          <div className="mt-0.5 text-[11px] leading-4 text-zinc-500 dark:text-zinc-400">
            根据操作系统外观自动在亮色和暗色主题之间切换
          </div>
        </div>
        <ToggleSwitch
          checked={themePreference === 'system'}
          onChange={(checked) => setTheme(checked ? 'system' : 'default-light')}
          accent="violet"
          title="跟随系统主题"
        />
      </div>

      {/* Light themes */}
      <div>
        <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-zinc-500">亮色主题</div>
        <div className="grid grid-cols-3 gap-2">
          {lightThemes.map(theme => (
            <ThemeCard
              key={theme.id}
              theme={theme}
              active={themePreference === theme.id}
              onSelect={() => setTheme(theme.id)}
            />
          ))}
        </div>
      </div>

      {/* Dark themes */}
      <div>
        <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-zinc-500">暗色主题</div>
        <div className="grid grid-cols-3 gap-2">
          {darkThemes.map(theme => (
            <ThemeCard
              key={theme.id}
              theme={theme}
              active={themePreference === theme.id}
              onSelect={() => setTheme(theme.id)}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

function ThemeCard({
  theme,
  active,
  onSelect,
}: {
  theme: { id: string; label: string; mode: string; colors: Record<string, string> };
  active: boolean;
  onSelect: () => void;
}) {
  const bg = theme.colors['--bg-primary'];
  const secondary = theme.colors['--bg-secondary'];
  const tertiary = theme.colors['--bg-tertiary'];
  const text = theme.colors['--text-primary'];
  const accent = theme.colors['--accent'];

  return (
    <button
      onClick={onSelect}
      className={`group relative overflow-hidden rounded-xl border p-0.5 transition-all ${
        active
          ? 'border-violet-400 ring-2 ring-violet-500/20 dark:border-violet-500'
          : 'border-zinc-200/80 hover:border-zinc-300 dark:border-zinc-700 dark:hover:border-zinc-600'
      }`}
    >
      {/* Mini preview */}
      <div
        className="rounded-[10px] p-2"
        style={{ backgroundColor: `rgb(${bg})` }}
      >
        {/* Title bar */}
        <div className="flex items-center gap-1 mb-1.5">
          <div className="flex gap-0.5">
            <span className="h-1.5 w-1.5 rounded-full" style={{ backgroundColor: `rgb(${accent})`, opacity: 0.7 }} />
            <span className="h-1.5 w-1.5 rounded-full" style={{ backgroundColor: `rgb(${text})`, opacity: 0.15 }} />
            <span className="h-1.5 w-1.5 rounded-full" style={{ backgroundColor: `rgb(${text})`, opacity: 0.15 }} />
          </div>
        </div>
        {/* Content lines */}
        <div className="flex gap-1">
          <div className="w-6 rounded" style={{ backgroundColor: `rgb(${secondary})`, height: '20px' }} />
          <div className="flex-1 space-y-1">
            <div className="h-1.5 w-3/4 rounded-full" style={{ backgroundColor: `rgb(${text})`, opacity: 0.2 }} />
            <div className="h-1.5 w-1/2 rounded-full" style={{ backgroundColor: `rgb(${accent})`, opacity: 0.35 }} />
            <div className="h-1.5 w-2/3 rounded-full" style={{ backgroundColor: `rgb(${text})`, opacity: 0.12 }} />
          </div>
        </div>
        {/* Bottom bar */}
        <div className="mt-1.5 h-2.5 rounded" style={{ backgroundColor: `rgb(${tertiary})` }} />
      </div>
      <div className="px-1.5 py-1.5 text-center">
        <span className={`text-[10px] font-medium ${active ? 'text-violet-600 dark:text-violet-400' : 'text-zinc-600 dark:text-zinc-400'}`}>
          {theme.label}
        </span>
      </div>
      {active && (
        <div className="absolute right-1.5 top-1.5">
          <CheckCircle2 className="h-3.5 w-3.5 text-violet-500" />
        </div>
      )}
    </button>
  );
}

// ── Tab: Shortcuts ───────────────────────────────────────────────────────────

function ShortcutsTab() {
  const shortcuts = [
    { keys: 'Enter', action: '发送消息' },
    { keys: 'Shift + Enter', action: '换行' },
    { keys: 'Esc', action: '关闭弹窗 / 取消' },
  ];

  return (
    <div className="space-y-5">
      <SectionHeading title="键盘快捷键" description="常用操作快捷键一览" />

      <div className="rounded-xl border border-zinc-200/60 bg-white/40 overflow-hidden dark:border-zinc-800/60 dark:bg-zinc-900/30">
        {shortcuts.map(({ keys, action }, i) => (
          <div
            key={keys}
            className={`flex items-center justify-between px-4 py-3 ${
              i !== shortcuts.length - 1 ? 'border-b border-zinc-200/40 dark:border-zinc-800/40' : ''
            }`}
          >
            <span className="text-sm text-zinc-700 dark:text-zinc-300">{action}</span>
            <div className="flex items-center gap-1">
              {keys.split(' + ').map((key) => (
                <kbd
                  key={key}
                  className="rounded-md border border-zinc-200/80 bg-zinc-100/80 px-2 py-0.5 font-mono text-[11px] text-zinc-600 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-400"
                >
                  {key}
                </kbd>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

// ── Shared sub-components ────────────────────────────────────────────────────

function SectionHeading({ title, description }: { title: string; description?: string }) {
  return (
    <div>
      <h3 className="text-[13px] font-semibold text-zinc-800 dark:text-zinc-100">{title}</h3>
      {description && (
        <p className="mt-0.5 text-[11px] leading-4 text-zinc-500 dark:text-zinc-400">{description}</p>
      )}
    </div>
  );
}

function InfoBanner({
  variant,
  children,
}: {
  variant: 'info' | 'warning' | 'success';
  children: React.ReactNode;
}) {
  const styles = {
    info: 'border-sky-200/70 bg-sky-50/70 text-sky-800 dark:border-sky-500/20 dark:bg-sky-500/10 dark:text-sky-200',
    warning: 'border-amber-200/70 bg-amber-50/70 text-amber-800 dark:border-amber-500/20 dark:bg-amber-500/10 dark:text-amber-200',
    success: 'border-emerald-200/70 bg-emerald-50/70 text-emerald-800 dark:border-emerald-500/20 dark:bg-emerald-500/10 dark:text-emerald-200',
  };

  return (
    <div className={`rounded-xl border px-4 py-2.5 text-[11px] leading-5 ${styles[variant]}`}>
      {children}
    </div>
  );
}
