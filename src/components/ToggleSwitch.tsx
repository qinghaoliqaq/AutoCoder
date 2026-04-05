interface ToggleSwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  accent?: 'violet' | 'emerald' | 'amber';
  title?: string;
}

export default function ToggleSwitch({
  checked,
  onChange,
  disabled = false,
  accent = 'violet',
  title,
}: ToggleSwitchProps) {
  const accentClasses = checked
    ? accent === 'emerald'
      ? 'border-emerald-300/70 bg-gradient-to-r from-emerald-400 to-teal-400 shadow-[0_0_0_4px_rgba(16,185,129,0.12)] dark:border-emerald-400/40'
      : accent === 'amber'
        ? 'border-amber-300/70 bg-gradient-to-r from-amber-400 to-rose-400 shadow-[0_0_0_4px_rgba(251,191,36,0.12)] dark:border-amber-400/40'
        : 'border-violet-300/70 bg-gradient-to-r from-violet-500 to-indigo-500 shadow-[0_0_0_4px_rgba(139,92,246,0.12)] dark:border-violet-400/40'
    : 'border-zinc-300/80 bg-zinc-200/75 dark:border-zinc-700 dark:bg-zinc-800';

  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      title={title}
      className={`relative inline-flex h-8 w-14 items-center rounded-full border transition-all duration-200 ${accentClasses} ${disabled ? 'cursor-not-allowed opacity-55' : 'cursor-pointer hover:scale-[1.02] active:scale-[0.98]'}`}
    >
      <span
        className={`absolute left-1 top-1/2 h-6 w-6 -translate-y-1/2 rounded-full bg-white shadow-[0_6px_18px_rgba(15,23,42,0.22)] transition-transform duration-200 ${checked ? 'translate-x-6' : 'translate-x-0'}`}
      />
    </button>
  );
}
