import { ConfigStatus } from '../types';
import ToggleSwitch from './ToggleSwitch';

type ExecutionAccessMode = ConfigStatus['execution_access_mode'];

interface AccessModeToggleProps {
  mode: ExecutionAccessMode;
  disabled?: boolean;
  onChange: (mode: ExecutionAccessMode) => void;
  compact?: boolean;
}

export default function AccessModeToggle({
  mode,
  disabled = false,
  onChange,
  compact = false,
}: AccessModeToggleProps) {
  const isFullAccess = mode === 'full_access';
  const containerClasses = compact
    ? 'gap-1.5 rounded-xl px-1.5 py-1 text-[10px]'
    : 'gap-2 rounded-2xl px-2.5 py-2 text-xs';
  const shellClasses = compact
    ? 'border-white/45 bg-white/20 shadow-[0_8px_20px_rgba(15,23,42,0.05)] dark:bg-zinc-900/30'
    : 'border-white/50 bg-white/30 shadow-[0_12px_30px_rgba(15,23,42,0.06)] dark:bg-zinc-900/40';
  const switchScale = compact ? 'scale-[0.92]' : '';

  return (
    <div
      className={`inline-flex items-center border backdrop-blur-xl dark:border-white/10 dark:shadow-[0_12px_30px_rgba(0,0,0,0.24)] ${containerClasses} ${shellClasses}`}
    >
      <div className="min-w-0">
        <div className={`font-semibold text-zinc-700 dark:text-zinc-200 ${compact ? 'text-[10px]' : ''}`}>
          {compact ? 'Access' : 'Execution Access'}
        </div>
        {!compact && (
          <div className="mt-0.5 text-[11px] leading-4 text-zinc-500 dark:text-zinc-400">
            {isFullAccess ? '无限制写入与命令执行' : '受限执行，默认更安全'}
          </div>
        )}
      </div>

      <div className={`flex items-center ${compact ? 'gap-1.5' : 'gap-2'}`}>
        <span
          className={`transition-colors ${compact ? 'text-[10px] font-semibold' : 'font-medium'} ${!isFullAccess ? 'text-zinc-900 dark:text-zinc-100' : 'text-zinc-400 dark:text-zinc-500'}`}
        >
          Sandbox
        </span>
        <div className={switchScale}>
          <ToggleSwitch
            checked={isFullAccess}
            accent="amber"
            disabled={disabled}
            onChange={(checked) => onChange(checked ? 'full_access' : 'sandbox')}
            title={isFullAccess ? '切换到 Sandbox' : '切换到 Full Access'}
          />
        </div>
        <span
          className={`transition-colors ${compact ? 'text-[10px] font-semibold' : 'font-medium'} ${isFullAccess ? 'text-zinc-900 dark:text-zinc-100' : 'text-zinc-400 dark:text-zinc-500'}`}
        >
          Full
        </span>
      </div>
    </div>
  );
}
