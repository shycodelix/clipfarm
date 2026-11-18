import {
  AlertTriangle,
  ArrowUpRight,
  Check,
  ChevronRight,
  Command,
  Download,
  FolderOpen,
  Info,
  Keyboard,
  Monitor,
  RefreshCw,
  Search,
  Trash2,
  X
} from 'lucide-react'
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode
} from 'react'
import type { Modal } from '../store'
import { api } from '../lib/bridge'
import { COMMANDS, type CommandDef } from '../lib/commands'
import { bytes, clock, shortcut as shortcutLabel } from '../lib/format'
import { totalDuration } from '../lib/timeline'
import { useStudio } from '../store'
import {
  Badge,
  Button,
  Field,
  IconButton,
  Keycap,
  ProgressBar,
  Segmented,
  Spinner,
  TextInput,
  Toggle,
  cn
} from './primitives'

/* ------------------------------------------------------------------ modal */

function ModalShell({
  title,
  subtitle,
  onClose,
  children,
  footer,
  width = 'w-[560px]',
  icon
}: {
  title: string
  subtitle?: string
  onClose: () => void
  children: ReactNode
  footer?: ReactNode
  width?: string
  icon?: ReactNode
}): ReactNode {
  useEffect(() => {
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        onClose()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/35 px-4 pt-[10vh] backdrop-blur-[3px]"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className={cn(
          'max-h-[80vh] w-full overflow-hidden rounded-[13px] border border-separator bg-surface shadow-[var(--shadow-float)]',
          width
        )}
      >
        <header className="flex items-start gap-2.5 border-b border-separator px-4 py-3">
          {icon ? <span className="mt-0.5 text-tertiary">{icon}</span> : null}
          <div className="min-w-0 flex-1">
            <h2 className="text-[13.5px] font-semibold tracking-[-0.01em] text-label">{title}</h2>
            {subtitle ? <p className="mt-0.5 text-[11.5px] leading-relaxed text-secondary">{subtitle}</p> : null}
          </div>
          <IconButton label="Close" size="sm" onClick={onClose}>
            <X size={14} aria-hidden />
          </IconButton>
        </header>
        <div className="max-h-[58vh] overflow-y-auto px-4 py-3.5">{children}</div>
        {footer ? (
          <footer className="flex items-center justify-end gap-2 border-t border-separator bg-elevated/50 px-4 py-2.5">
            {footer}
          </footer>
        ) : null}
      </div>
    </div>
  )
}

/* ------------------------------------------------------------ new project */

export function NewProjectDialog(): ReactNode {
  const { actions } = useStudio()
  const [name, setName] = useState('')
  const [busy, setBusy] = useState(false)

  const submit = async (): Promise<void> => {
    if (busy) return
    setBusy(true)
    await actions.createProject(name || 'Untitled Project')
    setBusy(false)
  }

  return (
    <ModalShell
      title="New Project"
      subtitle="A project keeps the timeline, media references and captions together."
      onClose={() => actions.setModal(null)}
      width="w-[440px]"
      footer={
        <>
          <Button variant="ghost" onClick={() => actions.setModal(null)}>
            Cancel
          </Button>
          <Button variant="primary" loading={busy} onClick={() => void submit()} disabled={busy}>
            Create
          </Button>
        </>
      }
    >
      <Field label="Project name">
        <input
          autoFocus
          value={name}
          onChange={(event) => setName(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') void submit()
          }}
          placeholder="Interview Cut — Episode 4"
          className="h-8 w-full select-text rounded-[7px] border border-separator bg-surface px-2.5 text-[12.5px] text-label placeholder:text-tertiary focus:border-accent/60 focus:outline-none"
        />
      </Field>
      <p className="mt-2 flex items-start gap-1.5 text-[11px] leading-relaxed text-tertiary">
        <Info size={12} aria-hidden className="mt-0.5 shrink-0" />
        Projects are stored locally as JSON. Media stays where it already lives on disk.
      </p>
    </ModalShell>
  )
}

export function DeleteProjectDialog({ projectId }: { projectId: string }): ReactNode {
  const { state, actions } = useStudio()
  const project = state.projects.find((item) => item.id === projectId)
  return (
    <ModalShell
      title="Delete this project?"
      subtitle="This removes the project file from clipfarm. Your original media stays untouched."
      onClose={() => actions.setModal(null)}
      width="w-[430px]"
      icon={<Trash2 size={15} aria-hidden />}
      footer={
        <>
          <Button variant="ghost" onClick={() => actions.setModal(null)}>
            Cancel
          </Button>
          <Button
            variant="danger"
            onClick={() => {
              void actions.deleteProject(projectId)
              actions.setModal(null)
            }}
          >
            Delete Project
          </Button>
        </>
      }
    >
      <div className="rounded-[8px] border border-separator bg-elevated px-3 py-2.5 text-[12px] text-label">
        {project?.name ?? 'Untitled Project'}
      </div>
    </ModalShell>
  )
}

/* ---------------------------------------------------------------- export */

const RESOLUTIONS = [
  { value: 'source', label: 'Source', detail: 'Keep original size' },
  { value: '1080p', label: '1080p', detail: '1920 × 1080' },
  { value: '4k', label: '4K', detail: '3840 × 2160' },
  { value: 'vertical', label: 'Vertical', detail: '1080 × 1920' }
] as const

export function ExportModal(): ReactNode {
  const { state, actions } = useStudio()
  const project = state.project
  const [resolution, setResolution] = useState<(typeof RESOLUTIONS)[number]['value']>('source')
  const [quality, setQuality] = useState<'high' | 'balanced' | 'small'>('balanced')
  const [burn, setBurn] = useState(Boolean(project?.settings.captionsEnabled))
  const [busy, setBusy] = useState(false)

  if (!project) return null
  const duration = totalDuration(project)
  const megabitsPerSecond = quality === 'high' ? 14 : quality === 'small' ? 4 : 7
  const estimate = bytes(
    (Math.max(duration, 1) * megabitsPerSecond * 1_000_000 * (resolution === '4k' ? 2.4 : 1)) / 8
  )
  const hasCaptions = project.captions.length > 0

  const start = async (): Promise<void> => {
    setBusy(true)
    try {
      await actions.startExport({
        container: 'mp4',
        resolution,
        quality,
        burnCaptions: burn && hasCaptions,
        fps: project.fps
      })
    } finally {
      setBusy(false)
    }
  }

  return (
    <ModalShell
      title="Export Video"
      subtitle="Rendered locally with FFmpeg — nothing uploads."
      onClose={() => actions.setModal(null)}
      width="w-[600px]"
      icon={<Download size={15} aria-hidden />}
      footer={
        <>
          <span className="mr-auto text-[10.5px] text-tertiary">
            {clock(duration)} · {project.fps} fps · H.264
          </span>
          <Button variant="ghost" onClick={() => actions.setModal(null)}>
            Cancel
          </Button>
          <Button variant="primary" loading={busy} onClick={() => void start()} disabled={project.clips.length === 0}>
            Choose destination…
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <section>
          <div className="mb-1.5 text-[11px] font-medium text-secondary">Resolution</div>
          <div className="grid grid-cols-4 gap-2">
            {RESOLUTIONS.map((option) => (
              <button
                key={option.value}
                type="button"
                onClick={() => setResolution(option.value)}
                aria-pressed={resolution === option.value}
                className={cn(
                  'flex flex-col items-start gap-0.5 rounded-[8px] border px-2.5 py-2 text-left transition-colors',
                  resolution === option.value
                    ? 'border-accent bg-accent-soft'
                    : 'border-separator bg-surface hover:bg-elevated'
                )}
              >
                <span className="text-[11.5px] font-semibold text-label">{option.label}</span>
                <span className="text-[10px] text-tertiary">{option.detail}</span>
              </button>
            ))}
          </div>
        </section>

        <section className="grid grid-cols-2 gap-4">
          <Field label="Quality">
            <Segmented
              label="Export quality"
              value={quality}
              onChange={setQuality}
              options={[
                { value: 'high', label: 'High' },
                { value: 'balanced', label: 'Balanced' },
                { value: 'small', label: 'Small' }
              ]}
            />
          </Field>
          <Field label="File">
            <div className="flex h-7 items-center rounded-[6px] border border-separator bg-elevated px-2 font-mono text-[11px] text-secondary">
              .mp4 · H.264 + AAC
            </div>
          </Field>
        </section>

        <section className="flex items-center justify-between rounded-[8px] border border-separator bg-elevated/40 px-3 py-2.5">
          <div>
            <div className="text-[11.5px] font-medium text-label">Burn in captions</div>
            <div className="text-[10.5px] text-tertiary">
              {hasCaptions
                ? `${project.captions.length} captions · ${project.settings.captionPreset} style`
                : 'No captions in this project yet'}
            </div>
          </div>
          <Toggle
            label="Burn in captions"
            checked={burn && hasCaptions}
            onChange={(value) => setBurn(value)}
          />
        </section>

        <div className="flex items-center justify-between rounded-[8px] border border-separator px-3 py-2 text-[11px]">
          <span className="text-secondary">Estimated size</span>
          <span className="font-mono text-label">{estimate}</span>
        </div>
      </div>
    </ModalShell>
  )
}

/* -------------------------------------------------------------- palette */

export function CommandPalette({ onRun }: { onRun: (id: string) => void }): ReactNode {
  const { actions } = useStudio()
  const [query, setQuery] = useState('')
  const [index, setIndex] = useState(0)
  const listRef = useRef<HTMLDivElement | null>(null)

  const results = useMemo(() => {
    const normalized = query.trim().toLowerCase()
    if (!normalized) return COMMANDS
    return COMMANDS.filter(
      (command) =>
        command.title.toLowerCase().includes(normalized) ||
        command.group.toLowerCase().includes(normalized) ||
        command.id.toLowerCase().includes(normalized)
    )
  }, [query])

  useEffect(() => {
    setIndex(0)
  }, [query])

  useEffect(() => {
    const item = listRef.current?.querySelector<HTMLElement>(`[data-index="${index}"]`)
    item?.scrollIntoView({ block: 'nearest' })
  }, [index])

  const run = (command: CommandDef): void => {
    actions.setModal(null)
    onRun(command.id)
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/30 px-4 pt-[12vh] backdrop-blur-[2px]"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) actions.setModal(null)
      }}
    >
      <div className="w-[540px] overflow-hidden rounded-[12px] border border-separator bg-surface shadow-[var(--shadow-float)]">
        <div className="flex items-center gap-2 border-b border-separator px-3 py-2.5">
          <Search size={14} aria-hidden className="text-tertiary" />
          <input
            autoFocus
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'ArrowDown') {
                event.preventDefault()
                setIndex((value) => Math.min(value + 1, results.length - 1))
              } else if (event.key === 'ArrowUp') {
                event.preventDefault()
                setIndex((value) => Math.max(value - 1, 0))
              } else if (event.key === 'Enter' && results[index]) {
                event.preventDefault()
                run(results[index])
              } else if (event.key === 'Escape') {
                actions.setModal(null)
              }
            }}
            placeholder="Search commands…"
            aria-label="Search commands"
            className="h-6 flex-1 select-text bg-transparent text-[13px] text-label placeholder:text-tertiary focus:outline-none"
          />
          <kbd className="rounded-[4px] border border-separator px-1 text-[10px] text-tertiary">esc</kbd>
        </div>
        <div ref={listRef} className="max-h-[340px] overflow-y-auto p-1.5">
          {results.length === 0 ? (
            <p className="px-3 py-6 text-center text-[12px] text-tertiary">No commands match “{query}”</p>
          ) : (
            results.map((command, position) => (
              <button
                key={command.id}
                type="button"
                data-index={position}
                onMouseEnter={() => setIndex(position)}
                onClick={() => run(command)}
                className={cn(
                  'flex w-full items-center gap-2 rounded-[6px] px-2 py-1.5 text-left',
                  position === index ? 'bg-accent text-white' : 'text-secondary hover:bg-elevated'
                )}
              >
                <span className={cn('w-[74px] shrink-0 text-[10px]', position === index ? 'text-white/70' : 'text-tertiary')}>
                  {command.group}
                </span>
                <span className={cn('flex-1 truncate text-[12px]', position === index ? 'text-white' : 'text-label')}>
                  {command.title}
                </span>
                {command.shortcut ? (
                  <span
                    className={cn(
                      'shrink-0 font-mono text-[10px]',
                      position === index ? 'text-white/80' : 'text-tertiary'
                    )}
                  >
                    {shortcutLabel(command.shortcut)}
                  </span>
                ) : null}
              </button>
            ))
          )}
        </div>
        <div className="flex items-center gap-3 border-t border-separator px-3 py-1.5 text-[10px] text-tertiary">
          <span className="flex items-center gap-1">
            <ChevronRight size={10} aria-hidden /> Run
          </span>
          <span>↑↓ navigate</span>
          <span className="ml-auto flex items-center gap-1">
            <Command size={10} aria-hidden /> K to reopen
          </span>
        </div>
      </div>
    </div>
  )
}

/* ------------------------------------------------------------ shortcuts */

export function ShortcutsSheet(): ReactNode {
  const { actions } = useStudio()
  const groups = useMemo(() => {
    const order = ['Project', 'Edit', 'AI', 'View', 'Help'] as const
    return order.map((group) => ({
      group,
      commands: COMMANDS.filter((command) => command.group === group && command.shortcut)
    }))
  }, [])

  return (
    <ModalShell
      title="Keyboard Shortcuts"
      subtitle="Every command is also in the menu bar."
      onClose={() => actions.setModal(null)}
      width="w-[540px]"
      icon={<Keyboard size={15} aria-hidden />}
    >
      <div className="grid grid-cols-2 gap-x-6 gap-y-4">
        {groups.map(({ group, commands }) => (
          <section key={group}>
            <h3 className="mb-1.5 text-[10.5px] font-semibold uppercase tracking-[0.06em] text-tertiary">{group}</h3>
            <ul className="flex flex-col gap-1">
              {commands.map((command) => (
                <li key={command.id} className="flex items-center justify-between gap-3">
                  <span className="truncate text-[11.5px] text-secondary">{command.title}</span>
                  <Keycap keys={command.shortcut ?? ''} />
                </li>
              ))}
            </ul>
          </section>
        ))}
      </div>
    </ModalShell>
  )
}

/* ------------------------------------------------------------- settings */

export function SettingsSheet(): ReactNode {
  const { state, actions, isDemo } = useStudio()
  const engine = state.engine
  const [checking, setChecking] = useState(false)

  const recheck = async (): Promise<void> => {
    setChecking(true)
    await actions.refreshEngine()
    setChecking(false)
  }

  return (
    <ModalShell
      title="Settings"
      subtitle="Appearance follows the system. The engine runs locally."
      onClose={() => actions.setModal(null)}
      width="w-[620px]"
      icon={<Monitor size={15} aria-hidden />}
    >
      <div className="flex flex-col gap-5">
        <section className="flex items-center justify-between rounded-[9px] border border-separator bg-elevated/40 px-3 py-2.5">
          <div>
            <div className="text-[12px] font-medium text-label">Appearance</div>
            <p className="mt-0.5 text-[11px] text-secondary">
              Follows the system Light/Dark appearance, including Reduce Transparency and Increase Contrast.
            </p>
          </div>
          <Badge tone="accent">Automatic</Badge>
        </section>

        <section>
          <div className="mb-1.5 flex items-center justify-between">
            <h3 className="text-[10.5px] font-semibold uppercase tracking-[0.06em] text-tertiary">Media engine</h3>
            <Button size="sm" variant="ghost" loading={checking} icon={<RefreshCw size={12} aria-hidden />} onClick={() => void recheck()}>
              Check again
            </Button>
          </div>
          <div className="overflow-hidden rounded-[9px] border border-separator">
            {[
              { label: 'FFmpeg', ready: Boolean(engine?.ffmpeg), detail: engine?.ffmpeg ?? 'Not found' },
              { label: 'FFprobe', ready: Boolean(engine?.ffprobe), detail: engine?.ffprobe ?? 'Not found' },
              {
                label: 'faster-whisper',
                ready: Boolean(engine?.packages.fasterWhisper),
                detail: engine?.packages.fasterWhisper ? 'installed' : 'optional'
              },
              {
                label: 'PySceneDetect',
                ready: Boolean(engine?.packages.scenedetect),
                detail: engine?.packages.scenedetect ? 'installed' : 'optional'
              },
              { label: 'Python', ready: Boolean(engine?.python), detail: engine?.pythonVersion ?? '—' },
              { label: 'Compute', ready: true, detail: engine?.device?.toUpperCase() ?? '—' }
            ].map((row) => (
              <div
                key={row.label}
                className="flex items-center gap-2 border-b border-separator px-3 py-1.5 last:border-b-0"
              >
                <span
                  className={cn(
                    'h-1.5 w-1.5 shrink-0 rounded-full',
                    row.ready ? 'bg-success' : 'bg-warning'
                  )}
                />
                <span className="w-[110px] text-[11.5px] text-label">{row.label}</span>
                <span className="min-w-0 flex-1 truncate text-right font-mono text-[10px] text-tertiary">
                  {row.detail}
                </span>
              </div>
            ))}
          </div>
          <p className="mt-1.5 text-[11px] leading-relaxed text-secondary">
            {isDemo
              ? 'Preview mode simulates analysis; the desktop build runs FFmpeg, Whisper and PySceneDetect locally.'
              : engine?.message}
          </p>
          <div className="mt-2 flex items-center gap-2">
            <code className="flex-1 truncate rounded-[6px] border border-separator bg-elevated px-2 py-1.5 font-mono text-[10.5px] text-secondary">
              npm run python:setup
            </code>
            <Button size="sm" onClick={() => void actions.copy('npm run python:setup', 'Install command')}>
              Copy
            </Button>
          </div>
        </section>

        <section className="grid grid-cols-2 gap-3">
          <div className="rounded-[9px] border border-separator px-3 py-2.5">
            <div className="text-[12px] font-medium text-label">Local data</div>
            <p className="mt-0.5 text-[10.5px] leading-relaxed text-secondary">
              Projects, thumbnails and model caches live in the app data folder.
            </p>
            <Button
              size="sm"
              className="mt-2"
              icon={<FolderOpen size={12} aria-hidden />}
              onClick={() => void api.openDataFolder()}
            >
              Show folder
            </Button>
          </div>
          <div className="rounded-[9px] border border-separator px-3 py-2.5">
            <div className="text-[12px] font-medium text-label">Agent bridge</div>
            <p className="mt-0.5 text-[10.5px] leading-relaxed text-secondary">
              Let an MCP client edit this timeline with plain language.
            </p>
            <Button
              size="sm"
              className="mt-2"
              icon={<ArrowUpRight size={12} aria-hidden />}
              onClick={() => {
                actions.setModal(null)
                actions.setView('agents')
              }}
            >
              Open setup
            </Button>
          </div>
        </section>

        <section className="flex items-center justify-between border-t border-separator pt-3 text-[10.5px] text-tertiary">
          <span>clipfarm {api.versions.app}</span>
          <span className="font-mono">
            Electron {api.versions.electron} · Chromium {api.versions.chrome}
          </span>
        </section>
      </div>
    </ModalShell>
  )
}

/* ---------------------------------------------------------------- toasts */

function ToastItem({ toast }: { toast: { id: string; kind: string; message: string; detail?: string } }): ReactNode {
  const { actions } = useStudio()
  useEffect(() => {
    const timer = window.setTimeout(() => actions.dismissToast(toast.id), 6200)
    return () => window.clearTimeout(timer)
  }, [toast.id, actions])

  const tone =
    toast.kind === 'error'
      ? { icon: <AlertTriangle size={14} aria-hidden className="text-danger" />, ring: 'border-[color-mix(in_srgb,var(--danger)_35%,var(--separator))]' }
      : toast.kind === 'success'
        ? { icon: <Check size={14} aria-hidden className="text-success" />, ring: 'border-[color-mix(in_srgb,var(--success)_30%,var(--separator))]' }
        : { icon: <Info size={14} aria-hidden className="text-accent" />, ring: 'border-separator' }

  return (
    <div
      role={toast.kind === 'error' ? 'alert' : 'status'}
      className={cn(
        'pointer-events-auto flex w-[320px] items-start gap-2.5 rounded-[10px] border bg-surface/95 px-3 py-2.5 shadow-[0_10px_30px_rgba(0,0,0,0.18)] backdrop-blur',
        tone.ring
      )}
    >
      <span className="mt-[1px]">{tone.icon}</span>
      <div className="min-w-0 flex-1">
        <div className="text-[11.5px] font-medium leading-snug text-label">{toast.message}</div>
        {toast.detail ? (
          <div className="mt-0.5 break-words text-[10.5px] leading-relaxed text-secondary">{toast.detail}</div>
        ) : null}
      </div>
      <button
        type="button"
        aria-label="Dismiss"
        onClick={() => actions.dismissToast(toast.id)}
        className="mt-0.5 text-tertiary hover:text-label"
      >
        <X size={12} aria-hidden />
      </button>
    </div>
  )
}

export function Toasts(): ReactNode {
  const { state } = useStudio()
  if (!state.toasts.length) return null
  return (
    <div className="pointer-events-none fixed bottom-11 right-4 z-40 flex flex-col gap-2" aria-live="polite">
      {state.toasts.map((toast) => (
        <ToastItem key={toast.id} toast={toast} />
      ))}
    </div>
  )
}

/* ------------------------------------------------------------------- job */

export function JobBar(): ReactNode {
  const { state, actions } = useStudio()
  const job = state.job
  if (!job) return null
  const running = job.status === 'running' || job.status === 'queued'
  const exportResult = job.result?.kind === 'export' ? job.result : null

  return (
    <div className="flex h-10 shrink-0 items-center gap-3 border-t border-separator bg-surface px-3">
      {running ? (
        <Spinner className="text-accent" />
      ) : job.status === 'done' ? (
        <Check size={14} aria-hidden className="text-success" />
      ) : job.status === 'error' ? (
        <AlertTriangle size={14} aria-hidden className="text-danger" />
      ) : (
        <Info size={14} aria-hidden className="text-tertiary" />
      )}
      <div className="min-w-0">
        <div className="truncate text-[11.5px] font-medium text-label">
          {job.status === 'done'
            ? `${job.label} complete`
            : job.status === 'error'
              ? `${job.label} failed`
              : job.status === 'cancelled'
                ? `${job.label} cancelled`
                : job.label}
        </div>
        <div className="truncate text-[10px] text-tertiary">
          {job.status === 'error' ? job.error : job.stage ?? 'Working'}
        </div>
      </div>
      <div className="mx-2 max-w-[320px] flex-1">
        <ProgressBar value={job.progress} tone={job.status === 'error' ? 'danger' : 'accent'} />
      </div>
      <span className="w-9 text-right font-mono text-[10.5px] tabular-nums text-tertiary">
        {Math.round(job.progress * 100)}%
      </span>
      {running ? (
        <Button size="sm" variant="ghost" onClick={() => void actions.cancelJob()}>
          Cancel
        </Button>
      ) : exportResult ? (
        <Button size="sm" variant="ghost" onClick={() => void api.revealFile(exportResult.outputPath)}>
          Show file
        </Button>
      ) : null}
      <IconButton label="Dismiss" size="sm" onClick={actions.dismissJob}>
        <X size={13} aria-hidden />
      </IconButton>
    </div>
  )
}

/* ------------------------------------------------------------ status bar */

export function StatusBar(): ReactNode {
  const { state, isDemo } = useStudio()
  return (
    <footer className="flex h-7 shrink-0 items-center gap-3 border-t border-separator bg-surface px-3 text-[10px] text-tertiary">
      <span className="flex items-center gap-1.5">
        <span className={cn('h-1.5 w-1.5 rounded-full', isDemo ? 'bg-warning' : state.engine?.ready ? 'bg-success' : 'bg-danger')} />
        {isDemo ? 'Preview mode' : state.engine?.ready ? 'Local engine' : 'Engine offline'}
      </span>
      {state.project ? (
        <>
          <span aria-hidden>·</span>
          <span className="truncate">{state.project.name}</span>
          <span aria-hidden>·</span>
          <span>{clock(totalDuration(state.project))}</span>
          <span aria-hidden>·</span>
          <span>{state.project.captions.length} captions</span>
        </>
      ) : null}
      <span className="ml-auto flex items-center gap-3">
        <span className="flex items-center gap-1">
          <Command size={10} aria-hidden /> K Commands
        </span>
        <span className="flex items-center gap-1">
          <Keyboard size={10} aria-hidden /> / Shortcuts
        </span>
      </span>
    </footer>
  )
}

export function renderModal(modal: Modal, onRun: (id: string) => void): ReactNode {
  if (!modal) return null
  switch (modal.kind) {
    case 'export':
      return <ExportModal />
    case 'palette':
      return <CommandPalette onRun={onRun} />
    case 'shortcuts':
      return <ShortcutsSheet />
    case 'settings':
      return <SettingsSheet />
    case 'new-project':
      return <NewProjectDialog />
    case 'delete-project':
      return <DeleteProjectDialog projectId={modal.projectId} />
    default:
      return null
  }
}
