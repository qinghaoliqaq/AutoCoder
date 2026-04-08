import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ConfigDraft, AGENT_PROVIDERS } from '../types';
import AccessModeToggle from './AccessModeToggle';
import ToggleSwitch from './ToggleSwitch';
import ProviderSelect from './ProviderSelect';
import { useTheme, THEMES } from './ThemeProvider';
import { CheckCircle2, AlertTriangle, LoaderCircle, Settings2, Bot, Keyboard, Zap, Palette, Lightbulb } from 'lucide-react';

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
        <span className="text-[11px] font-semibold uppercase tracking-[0.08em] text-content-tertiary">{label}</span>
        {hint && <span className="text-[10px] font-normal normal-case text-content-tertiary/70">{hint}</span>}
      </div>
      {children}
    </label>
  );
}

const inputClass =
  'w-full rounded-xl border border-edge-primary/60 bg-surface-input px-3 py-2.5 text-sm text-content-primary outline-none transition-all duration-200 placeholder:text-content-tertiary focus:border-themed-accent/50 focus:ring-2 focus:ring-themed-accent/10 focus:shadow-[0_0_0_3px_rgb(var(--accent)/0.06)]';

// ── Tips carousel ────────────────────────────────────────────────────────────

const TIPS = [
  'Full Access 模式下 Claude / Codex 会跳过权限限制，执行更激进但风险更高。',
  'Sandbox 模式下优先使用受限权限，审查型任务保持只读，更安全。',
  'Plan 模式会自动将任务拆分为子任务，并行执行以提高效率。',
  'Build Gate 会在代码提交前自动进行编译/类型检查，提前发现错误。',
  '可以在项目根目录放置 .autocoder/skills/ 来注入自定义 Vendored Skills。',
  'Director 支持 Anthropic 和 OpenAI 兼容 API，可切换不同模型。',
  '快捷键 Cmd+1~4 可以快速切换设置页面的标签。',
  '调整 Parallel Lanes 可以控制同时执行的子任务数量，推荐 2-4 个。',
];

function TipsCarousel() {
  const [displayIndex, setDisplayIndex] = useState(() => Math.floor(Math.random() * TIPS.length));
  const [fadingOut, setFadingOut] = useState(false);
  // nextIndex is the tip waiting to be shown after fade-out completes
  const nextIndex = (displayIndex + 1) % TIPS.length;

  useEffect(() => {
    const timer = setInterval(() => {
      // Start fade-out
      setFadingOut(true);
    }, 5000);
    return () => clearInterval(timer);
  }, [displayIndex]);

  const handleTransitionEnd = () => {
    if (fadingOut) {
      // Fade-out done — swap content and fade back in
      setDisplayIndex(nextIndex);
      setFadingOut(false);
    }
  };

  return (
    <div className="flex items-start gap-2.5 rounded-xl border border-edge-primary/30 bg-surface-secondary/30 px-4 py-2.5">
      <Lightbulb className="h-3.5 w-3.5 mt-0.5 flex-shrink-0 text-amber-500/70" />
      <span
        className={`text-[11px] leading-5 text-content-secondary transition-opacity duration-300 ${fadingOut ? 'opacity-0' : 'opacity-100'}`}
        onTransitionEnd={handleTransitionEnd}
      >
        {TIPS[displayIndex]}
      </span>
    </div>
  );
}

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
    <div className="fixed inset-0 z-50 flex flex-col bg-surface-primary">
      {/* ── Top bar (pl-24 avoids macOS traffic-light buttons) ──── */}
      <div className="flex items-center justify-between border-b border-edge-primary/30 pl-24 pr-5 py-3">
        <div className="flex items-center gap-2.5">
          <Settings2 className="h-4 w-4 text-content-tertiary" />
          <h1 className="text-sm font-semibold text-content-primary">Settings</h1>
          <span className="text-[11px] text-content-tertiary">
            保存后写入 <code className="rounded bg-surface-tertiary px-1 py-0.5 text-[10px]">config.toml</code>
          </span>
        </div>
        <button
          onClick={onClose}
          className="rounded-lg p-1.5 text-content-tertiary transition-colors hover:bg-surface-tertiary hover:text-content-primary"
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
        <nav className="w-44 flex-shrink-0 border-r border-edge-primary/30 px-3 py-4 space-y-1">
          {TABS.map(({ id, label, icon }) => (
            <button
              key={id}
              onClick={() => setActiveTab(id)}
              className={`flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-[13px] font-medium transition-colors ${
                activeTab === id
                  ? 'bg-themed-accent-soft/70 text-themed-accent-text'
                  : 'text-content-secondary hover:bg-surface-tertiary/40 hover:text-content-primary'
              }`}
            >
              {icon}
              {label}
            </button>
          ))}
        </nav>

        {/* Content area — only scrolls when content exceeds viewport */}
        <div className="flex-1 custom-scrollbar overflow-y-auto min-h-0">
          {draft ? (
            <div className="mx-auto max-w-2xl px-8 py-4">
              {activeTab === 'general' && <GeneralTab draft={draft} update={update} />}
              {activeTab === 'agent' && <AgentTab draft={draft} update={update} />}
              {activeTab === 'appearance' && <AppearanceTab />}
              {activeTab === 'shortcuts' && <ShortcutsTab />}

              {error && (
                <div className="mt-4 rounded-xl border border-rose-500/20 bg-rose-500/10 px-4 py-3 text-xs leading-5 text-rose-600">
                  {error}
                </div>
              )}
            </div>
          ) : error ? (
            <div className="mx-auto max-w-2xl px-8 py-6">
              <div className="rounded-xl border border-rose-500/20 bg-rose-500/10 px-4 py-3 text-xs leading-5 text-rose-600">
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
      <div className="flex items-center justify-between gap-3 border-t border-edge-primary/30 px-6 py-3">
        <div className="text-[11px] text-content-tertiary">
          保存后新对话将使用最新配置
        </div>
        <div className="flex gap-2">
          <button
            onClick={onClose}
            className="rounded-lg px-4 py-1.5 text-xs font-medium text-content-secondary glass-button"
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
    <div className="space-y-4">
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
            placeholder="gpt-4o / claude-sonnet-4-0 / MiniMax-M1"
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
          className="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium text-themed-accent-text glass-button disabled:cursor-not-allowed disabled:opacity-50"
        >
          {testStatus === 'testing' ? (
            <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <Zap className="h-3.5 w-3.5" />
          )}
          {testStatus === 'testing' ? '测试中...' : '测试连通性'}
        </button>

        {testStatus === 'success' && (
          <span className="flex items-center gap-1 text-[11px] font-medium text-emerald-600">
            <CheckCircle2 className="h-3.5 w-3.5" /> {testMessage}
          </span>
        )}
        {testStatus === 'error' && (
          <span className="flex items-center gap-1 text-[11px] font-medium text-rose-600">
            <AlertTriangle className="h-3.5 w-3.5" /> {testMessage}
          </span>
        )}
      </div>

      <div className="h-px bg-edge-primary/40" />

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
      <div className="flex items-center justify-between rounded-xl border border-edge-primary/40 bg-surface-elevated/40 px-4 py-3">
        <div>
          <div className="text-sm font-medium text-content-primary">Vendored Skills</div>
          <div className="mt-0.5 text-[11px] leading-4 text-content-tertiary">
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

      <TipsCarousel />
    </div>
  );
}

// ── Tab: Agent ───────────────────────────────────────────────────────────────

type ConnectionStatus = 'idle' | 'testing' | 'success' | 'error';

interface AgentIdentityValues {
  provider: string;
  apiKey: string;
  baseUrl: string;
  model: string;
}

function resolvePrimaryIdentity(draft: ConfigDraft): AgentIdentityValues {
  return {
    provider: draft.agent_provider,
    apiKey: draft.agent_api_key,
    baseUrl: draft.agent_base_url,
    model: draft.agent_model,
  };
}

function resolveSecondIdentity(draft: ConfigDraft): AgentIdentityValues {
  return {
    provider: draft.agent_second_provider || draft.agent_provider,
    apiKey: draft.agent_second_api_key || draft.agent_api_key,
    baseUrl: draft.agent_second_base_url || draft.agent_base_url,
    model: draft.agent_second_model || draft.agent_model,
  };
}

function AgentTab({
  draft,
  update,
}: {
  draft: ConfigDraft;
  update: <K extends keyof ConfigDraft>(key: K, value: ConfigDraft[K]) => void;
}) {
  const primary = resolvePrimaryIdentity(draft);
  const second = resolveSecondIdentity(draft);

  return (
    <div className="space-y-5">
      <SectionHeading
        title="智能体执行层"
        description="配置后，技能（代码、调试、测试等）将通过 API 直接执行。本地工具（Bash、编辑器等）完全免费运行。"
      />

      <AgentIdentityCard
        title="主身份 (Claude)"
        description="编码、方案起草阶段使用的模型。请参考右侧链接中的模型名称手动输入。"
        raw={primary}
        effective={primary}
        providerHint={undefined}
        apiKeyHint={undefined}
        modelHint="留空使用供应商默认模型"
        baseUrlHint="留空使用供应商默认端点"
        onProviderChange={(value) => update('agent_provider', value)}
        onApiKeyChange={(value) => update('agent_api_key', value)}
        onModelChange={(value) => update('agent_model', value)}
        onBaseUrlChange={(value) => update('agent_base_url', value)}
      />

      <AgentIdentityCard
        title="副身份 (Codex)"
        description="审阅、诊断、测试、评估阶段使用的模型。留空则自动跟随主身份。"
        raw={{
          provider: draft.agent_second_provider,
          apiKey: draft.agent_second_api_key,
          baseUrl: draft.agent_second_base_url,
          model: draft.agent_second_model,
        }}
        effective={second}
        providerHint="留空跟随主身份"
        apiKeyHint="留空跟随主身份 Key"
        modelHint="留空跟随主身份模型"
        baseUrlHint="留空跟随主身份端点"
        onProviderChange={(value) => update('agent_second_provider', value)}
        onApiKeyChange={(value) => update('agent_second_api_key', value)}
        onModelChange={(value) => update('agent_second_model', value)}
        onBaseUrlChange={(value) => update('agent_second_base_url', value)}
      />

    </div>
  );
}

function AgentIdentityCard({
  title,
  description,
  raw,
  effective,
  providerHint,
  apiKeyHint,
  modelHint,
  baseUrlHint,
  onProviderChange,
  onApiKeyChange,
  onModelChange,
  onBaseUrlChange,
}: {
  title: string;
  description: string;
  raw: AgentIdentityValues;
  effective: AgentIdentityValues;
  providerHint?: string;
  apiKeyHint?: string;
  modelHint?: string;
  baseUrlHint?: string;
  onProviderChange: (value: string) => void;
  onApiKeyChange: (value: string) => void;
  onModelChange: (value: string) => void;
  onBaseUrlChange: (value: string) => void;
}) {
  const [testStatus, setTestStatus] = useState<ConnectionStatus>('idle');
  const [testMessage, setTestMessage] = useState('');

  useEffect(() => {
    setTestStatus('idle');
    setTestMessage('');
  }, [effective.provider, effective.apiKey, effective.baseUrl, effective.model]);

  const handleTestConnection = async () => {
    if (!effective.provider || !effective.apiKey) return;
    setTestStatus('testing');
    setTestMessage('');
    try {
      const result = await invoke<string>('test_agent_connection', {
        provider: effective.provider,
        apiKey: effective.apiKey,
        baseUrl: effective.baseUrl,
        model: effective.model,
      });
      setTestStatus('success');
      setTestMessage(result);
    } catch (err) {
      setTestStatus('error');
      setTestMessage(String(err));
    }
  };

  // Doc link for the selected provider
  const selectedProvider = AGENT_PROVIDERS.find(p => p.value === effective.provider);
  const docUrl = selectedProvider?.doc_url ?? null;

  return (
    <div className="space-y-4 rounded-2xl border border-edge-primary/40 bg-surface-elevated/20 px-4 py-4">
      <SectionHeading title={title} description={description} />

      <div className="grid gap-4 sm:grid-cols-2">
        <FieldGroup label="供应商" hint={providerHint}>
          <div className="flex items-center gap-2">
            <div className="flex-1">
              <ProviderSelect
                value={raw.provider}
                onChange={onProviderChange}
              />
            </div>
            {docUrl && (
              <a
                href={docUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="flex-shrink-0 text-[11px] text-sky-500 hover:text-sky-400 transition-colors"
                title="查看模型列表"
              >
                📄 模型文档
              </a>
            )}
          </div>
        </FieldGroup>

        <FieldGroup label="API Key" hint={apiKeyHint}>
          <input
            type="password"
            value={raw.apiKey}
            onChange={(e) => onApiKeyChange(e.target.value)}
            placeholder={raw.provider || effective.provider ? '输入供应商 API Key' : '请先选择供应商'}
            disabled={!raw.provider && !effective.provider}
            className={`${inputClass} disabled:opacity-50 disabled:cursor-not-allowed`}
          />
        </FieldGroup>

        <FieldGroup label="Model" hint={modelHint}>
          <input
            type="text"
            value={raw.model}
            onChange={(e) => onModelChange(e.target.value)}
            placeholder="请手动输入模型名称"
            disabled={!raw.provider && !effective.provider}
            className={`${inputClass} disabled:opacity-50 disabled:cursor-not-allowed`}
          />
        </FieldGroup>

        <FieldGroup label="Base URL" hint={baseUrlHint}>
          <input
            type="text"
            value={raw.baseUrl}
            onChange={(e) => onBaseUrlChange(e.target.value)}
            placeholder="使用供应商默认端点"
            disabled={!raw.provider && !effective.provider}
            className={`${inputClass} disabled:opacity-50 disabled:cursor-not-allowed`}
          />
        </FieldGroup>
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <button
          type="button"
          onClick={handleTestConnection}
          disabled={testStatus === 'testing' || !effective.provider || !effective.apiKey}
          className="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium text-themed-accent-text glass-button disabled:cursor-not-allowed disabled:opacity-50"
        >
          {testStatus === 'testing' ? (
            <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <Zap className="h-3.5 w-3.5" />
          )}
          {testStatus === 'testing' ? '测试中...' : '测试连通性'}
        </button>

        {testStatus === 'success' && (
          <span className="flex items-center gap-1 text-[11px] font-medium text-emerald-600">
            <CheckCircle2 className="h-3.5 w-3.5" /> {testMessage}
          </span>
        )}
        {testStatus === 'error' && (
          <span className="flex items-center gap-1 text-[11px] font-medium text-rose-600">
            <AlertTriangle className="h-3.5 w-3.5" /> {testMessage}
          </span>
        )}
      </div>

      {!effective.provider && (
        <InfoBanner variant="info">
          请从上方选择供应商，并在模型文档链接中确认可用的模型名称。
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
      <div className="flex items-center justify-between rounded-xl border border-edge-primary/40 bg-surface-elevated/40 px-4 py-3">
        <div>
          <div className="text-sm font-medium text-content-primary">跟随系统</div>
          <div className="mt-0.5 text-[11px] leading-4 text-content-tertiary">
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
        <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-content-tertiary">亮色主题</div>
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
        <div className="mb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-content-tertiary">暗色主题</div>
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
          ? 'border-themed-accent ring-2 ring-themed-accent/20'
          : 'border-edge-primary/80 hover:border-edge-primary'
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
        <span className={`text-[10px] font-medium ${active ? 'text-themed-accent-text' : 'text-content-secondary'}`}>
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

      <div className="rounded-xl border border-edge-primary/40 bg-surface-elevated/40 overflow-hidden">
        {shortcuts.map(({ keys, action }, i) => (
          <div
            key={keys}
            className={`flex items-center justify-between px-4 py-3 ${
              i !== shortcuts.length - 1 ? 'border-b border-edge-primary/30' : ''
            }`}
          >
            <span className="text-sm text-content-secondary">{action}</span>
            <div className="flex items-center gap-1">
              {keys.split(' + ').map((key) => (
                <kbd
                  key={key}
                  className="rounded-md border border-edge-primary/60 bg-surface-tertiary/80 px-2 py-0.5 font-mono text-[11px] text-content-secondary"
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
      <h3 className="text-[13px] font-semibold text-content-primary">{title}</h3>
      {description && (
        <p className="mt-0.5 text-[11px] leading-4 text-content-tertiary">{description}</p>
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
    info: 'border-sky-500/20 bg-sky-500/10 text-sky-600',
    warning: 'border-amber-500/20 bg-amber-500/10 text-amber-600',
    success: 'border-emerald-500/20 bg-emerald-500/10 text-emerald-600',
  };

  return (
    <div className={`rounded-xl border px-4 py-2.5 text-[11px] leading-5 ${styles[variant]}`}>
      {children}
    </div>
  );
}
