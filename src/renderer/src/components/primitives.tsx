import { Loader2 } from 'lucide-react'
import type { ButtonHTMLAttributes, ReactNode } from 'react'
import { shortcut as shortcutLabel } from '../lib/format'

export function cn(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(' ')
}

type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger'

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant
  size?: 'sm' | 'md'
  icon?: ReactNode
  loading?: boolean
}

const BUTTON_VARIANTS: Record<ButtonVariant, string> = {
  primary: 'bg-accent text-white hover:brightness-110 shadow-[0_1px_2px_rgba(0,0,0,0.18)]',
  secondary: 'bg-surface text-label border border-separator hover:bg-elevated shadow-[0_1px_1px_rgba(0,0,0,0.05)]',
  ghost: 'text-secondary hover:text-label hover:bg-elevated',
  danger: 'bg-danger text-white hover:brightness-110'
}

export function Button({
  variant = 'secondary',
  size = 'md',
  icon,
  loading,
  className,
  children,
  ...rest
}: ButtonProps): ReactNode {
  return (
    <button
      type="button"
      className={cn(
        'inline-flex select-none items-center justify-center gap-1.5 rounded-[7px] font-medium transition-[background-color,filter,opacity] disabled:pointer-events-none disabled:opacity-40',
        size === 'md' ? 'h-8 px-3 text-[12.5px]' : 'h-7 px-2 text-[12px]',
        BUTTON_VARIANTS[variant],
        className
      )}
      {...rest}
    >
      {loading ? <Loader2 size={14} className="spin" aria-hidden /> : icon}
      {children}
    </button>
  )
}

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  label: string
  shortcut?: string
  active?: boolean
  size?: 'sm' | 'md'
}

export function IconButton({
  label,
  shortcut,
  active,
  size = 'md',
  className,
  children,
  ...rest
}: IconButtonProps): ReactNode {
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={active}
      title={shortcut ? `${label} (${shortcutLabel(shortcut)})` : label}
      className={cn(
        'inline-flex items-center justify-center rounded-[7px] text-secondary transition-colors hover:bg-elevated hover:text-label disabled:pointer-events-none disabled:opacity-35',
        active && 'bg-accent-soft text-accent',
        size === 'md' ? 'h-8 w-8' : 'h-7 w-7',
        className
      )}
      {...rest}
    >
      {children}
    </button>
  )
}

export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label
}: {
  value: T
  options: Array<{ value: T; label: string; icon?: ReactNode }>
  onChange: (value: T) => void
  label: string
}): ReactNode {
  return (
    <div
      role="tablist"
      aria-label={label}
      className="inline-flex items-center gap-0.5 rounded-[8px] border border-separator bg-surface p-0.5"
    >
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          role="tab"
          aria-selected={value === option.value}
          onClick={() => onChange(option.value)}
          className={cn(
            'inline-flex h-6 items-center gap-1.5 rounded-[6px] px-2.5 text-[11.5px] font-medium transition-colors',
            value === option.value
              ? 'bg-elevated text-label shadow-[0_1px_2px_rgba(0,0,0,0.08)]'
              : 'text-secondary hover:text-label'
          )}
        >
          {option.icon}
          {option.label}
        </button>
      ))}
    </div>
  )
}

export function Toggle({
  checked,
  onChange,
  label
}: {
  checked: boolean
  onChange: (checked: boolean) => void
  label: string
}): ReactNode {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={cn(
        'relative h-[22px] w-[38px] shrink-0 rounded-full transition-colors',
        checked ? 'bg-accent' : 'bg-[color-mix(in_srgb,var(--label)_18%,transparent)]'
      )}
    >
      <span
        className={cn(
          'absolute top-[2px] h-[18px] w-[18px] rounded-full bg-white shadow-[0_1px_3px_rgba(0,0,0,0.3)] transition-[left]',
          checked ? 'left-[18px]' : 'left-[2px]'
        )}
      />
    </button>
  )
}

export function Keycap({ keys }: { keys: string }): ReactNode {
  return (
    <kbd className="inline-flex h-[19px] min-w-[19px] items-center justify-center rounded-[4px] border border-separator bg-surface px-1 font-sans text-[10.5px] font-medium text-secondary shadow-[0_1px_0_var(--separator)]">
      {shortcutLabel(keys)}
    </kbd>
  )
}

export function Spinner({ className }: { className?: string }): ReactNode {
  return <Loader2 size={14} aria-hidden className={cn('spin', className)} />
}

export function Badge({
  children,
  tone = 'neutral'
}: {
  children: ReactNode
  tone?: 'neutral' | 'accent' | 'success' | 'warning' | 'danger'
}): ReactNode {
  const tones = {
    neutral: 'bg-elevated text-secondary border-separator',
    accent: 'bg-accent-soft text-accent border-transparent',
    success: 'bg-[color-mix(in_srgb,var(--success)_14%,transparent)] text-success border-transparent',
    warning: 'bg-[color-mix(in_srgb,var(--warning)_16%,transparent)] text-warning border-transparent',
    danger: 'bg-[color-mix(in_srgb,var(--danger)_14%,transparent)] text-danger border-transparent'
  }
  return (
    <span
      className={cn(
        'inline-flex h-[18px] items-center gap-1 rounded-full border px-1.5 text-[10.5px] font-semibold tracking-[0.01em]',
        tones[tone]
      )}
    >
      {children}
    </span>
  )
}

export function Field({
  label,
  hint,
  children
}: {
  label: string
  hint?: string
  children: ReactNode
}): ReactNode {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="flex items-baseline justify-between text-[11px] font-medium text-secondary">
        {label}
        {hint ? <span className="font-normal text-tertiary">{hint}</span> : null}
      </span>
      {children}
    </label>
  )
}

export function TextInput(props: React.InputHTMLAttributes<HTMLInputElement>): ReactNode {
  return (
    <input
      {...props}
      className={cn(
        'h-7 w-full select-text rounded-[6px] border border-separator bg-surface px-2 text-[12px] text-label placeholder:text-tertiary',
        props.className
      )}
    />
  )
}

export function EmptyState({
  icon,
  title,
  message,
  action,
  compact
}: {
  icon: ReactNode
  title: string
  message: string
  action?: ReactNode
  compact?: boolean
}): ReactNode {
  return (
    <div
      className={cn(
        'flex flex-col items-center justify-center gap-2 text-center',
        compact ? 'px-4 py-8' : 'px-6 py-16'
      )}
    >
      <div className="flex h-11 w-11 items-center justify-center rounded-[12px] border border-separator bg-surface text-tertiary shadow-[0_1px_2px_rgba(0,0,0,0.05)]">
        {icon}
      </div>
      <div className="text-[13px] font-semibold text-label">{title}</div>
      <p className="max-w-[260px] text-[11.5px] leading-relaxed text-secondary">{message}</p>
      {action ? <div className="mt-1.5">{action}</div> : null}
    </div>
  )
}

export function ProgressBar({
  value,
  indeterminate,
  tone = 'accent'
}: {
  value: number
  indeterminate?: boolean
  tone?: 'accent' | 'danger'
}): ReactNode {
  return (
    <div
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={indeterminate ? undefined : Math.round(value * 100)}
      className={cn(
        'relative h-1.5 w-full overflow-hidden rounded-full bg-[color-mix(in_srgb,var(--label)_12%,transparent)]',
        indeterminate && 'indeterminate-bar'
      )}
    >
      {indeterminate ? null : (
        <div
          className={cn('h-full rounded-full transition-[width] duration-200', tone === 'danger' ? 'bg-danger' : 'bg-accent')}
          style={{ width: `${Math.max(0, Math.min(100, value * 100))}%` }}
        />
      )}
    </div>
  )
}
