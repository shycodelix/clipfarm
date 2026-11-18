import {
  AudioLines,
  Captions as CaptionsIcon,
  Check,
  Cpu,
  Layers,
  Scissors,
  Sparkles,
  Trash2,
  Wand2,
  X
} from 'lucide-react'
import { useEffect, useState, type ReactNode } from 'react'
import type { Caption, Clip, Project } from '@shared/types'
import { isDemo } from '../lib/bridge'
import { clock, gradientFor, clamp } from '../lib/format'
import { deleteCaption, findClip, updateCaption } from '../lib/edits'
import { shortcut as shortcutLabel } from '../lib/format'
import { totalDuration } from '../lib/timeline'
import { useStudio } from '../store'
import { Badge, Button, EmptyState, Field, IconButton, Segmented, Toggle } from './primitives'

function NumberField({
  label,
  value,
  onCommit,
  min = 0,
  max = 36_000,
  step = 0.1,
  suffix = 's'
}: {
  label: string
  value: number
  onCommit: (value: number) => void
  min?: number
  max?: number
  step?: number
  suffix?: string
}): ReactNode {
  const [draft, setDraft] = useState<string | null>(null)
  const display = draft ?? value.toFixed(2)

  return (
    <label className="flex min-w-0 flex-col gap-1">
      <span className="text-[10.5px] font-medium text-secondary">{label}</span>
      <span className="relative flex items-center">
        <input
          value={display}
          inputMode="decimal"
          step={step}
          min={min}
          max={max}
          onChange={(event) => setDraft(event.target.value)}
          onFocus={(event) => {
            setDraft(value.toFixed(2))
            event.currentTarget.select()
          }}
          onBlur={() => {
            if (draft !== null) {
              const parsed = Number(draft.replace(',', '.'))
              if (Number.isFinite(parsed)) onCommit(clamp(parsed, min, max))
            }
            setDraft(null)
          }}
          onKeyDown={(event) => {
            if (event.key === 'Enter') event.currentTarget.blur()
            if (event.key === 'Escape') {
              setDraft(null)
              event.currentTarget.blur()
            }
          }}
          className="h-7 w-full select-text rounded-[6px] border border-separator bg-surface px-2 pr-4 font-mono text-[11.5px] tabular-nums text-label focus:border-accent/60 focus:outline-none"
        />
        <span className="pointer-events-none absolute right-1.5 text-[9.5px] text-tertiary">{suffix}</span>
      </span>
    </label>
  )
}

function SequenceInfo({ project }: { project: Project }): ReactNode {
  const duration = totalDuration(project)
  return (
    <div className="rounded-[8px] border border-separator bg-elevated/60 px-3 py-2.5">
      <div className="mb-1.5 text-[10.5px] font-semibold uppercase tracking-[0.05em] text-tertiary">Sequence</div>
      <dl className="grid grid-cols-2 gap-y-1 text-[11px]">
        <dt className="text-secondary">Resolution</dt>
        <dd className="text-right font-mono text-label">
          {project.width}×{project.height}
        </dd>
        <dt className="text-secondary">Frame rate</dt>
        <dd className="text-right font-mono text-label">{project.fps} fps</dd>
        <dt className="text-secondary">Duration</dt>
        <dd className="text-right font-mono text-label">{clock(duration)}</dd>
        <dt className="text-secondary">Clips</dt>
        <dd className="text-right font-mono text-label">{project.clips.length}</dd>
      </dl>
    </div>
  )
}

function ClipPanel(): ReactNode {
  const { state, actions } = useStudio()
  const project = state.project
  const clip: Clip | undefined = project ? findClip(project, state.selectedClipId) : undefined

  if (!project) return null

  if (!clip) {
    return (
      <div className="flex flex-col gap-3">
        <EmptyState
          compact
          icon={<Layers size={17} aria-hidden />}
          title="Nothing selected"
          message="Select a clip on the timeline to trim it, change its speed or split it at the playhead."
        />
        <SequenceInfo project={project} />
      </div>
    )
  }

  const asset = project.assets.find((item) => item.id === clip.assetId)
  const [from, to] = gradientFor(asset?.name ?? 'clip')
  const track = project.tracks.find((item) => item.id === clip.trackId)

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center gap-2.5">
        <div
          className="h-9 w-14 shrink-0 rounded-[5px]"
          style={{ background: `linear-gradient(140deg, ${from}, ${to})` }}
        />
        <div className="min-w-0">
          <div className="truncate text-[12px] font-semibold text-label">{asset?.name ?? 'Missing media'}</div>
          <div className="mt-0.5 flex items-center gap-1.5 text-[10.5px] text-tertiary">
            <span>{track?.name ?? 'Track'}</span>
            <span aria-hidden>·</span>
            <span className="font-mono">{clock(clip.duration)}</span>
            {clip.speed !== 1 ? <Badge tone="accent">{clip.speed}×</Badge> : null}
          </div>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-2.5">
        <NumberField
          label="Start"
          value={clip.start}
          onCommit={(value) =>
            actions.mutate((draft) => {
              const target = findClip(draft, clip.id)
              if (target) target.start = value
            })
          }
        />
        <NumberField
          label="Duration"
          min={0.2}
          value={clip.duration}
          onCommit={(value) =>
            actions.mutate((draft) => {
              const target = findClip(draft, clip.id)
              if (target) target.duration = Math.max(0.2, value)
            })
          }
        />
        <NumberField
          label="Source in"
          value={clip.inPoint}
          onCommit={(value) =>
            actions.mutate((draft) => {
              const target = findClip(draft, clip.id)
              if (target) target.inPoint = value
            })
          }
        />
        <label className="flex flex-col gap-1">
          <span className="text-[10.5px] font-medium text-secondary">Track</span>
          <select
            value={clip.trackId}
            onChange={(event) =>
              actions.mutate((draft) => {
                const target = findClip(draft, clip.id)
                if (target) target.trackId = event.target.value
              })
            }
            className="h-7 appearance-none rounded-[6px] border border-separator bg-surface px-2 text-[11.5px] text-label focus:border-accent/60 focus:outline-none"
          >
            {project.tracks
              .filter((item) => item.kind !== 'captions')
              .map((item) => (
                <option key={item.id} value={item.id}>
                  {item.name}
                </option>
              ))}
          </select>
        </label>
      </div>

      <Field label="Speed" hint={`${clip.speed.toFixed(2)}×`}>
        <input
          type="range"
          min={0.25}
          max={2}
          step={0.05}
          value={clip.speed}
          onChange={(event) =>
            actions.mutate((draft) => {
              const target = findClip(draft, clip.id)
              if (target) target.speed = Number(event.target.value)
            })
          }
          aria-label="Clip speed"
        />
      </Field>

      <Field label="Volume" hint={`${Math.round(clip.volume * 100)}%`}>
        <input
          type="range"
          min={0}
          max={2}
          step={0.05}
          value={clip.volume}
          onChange={(event) =>
            actions.mutate((draft) => {
              const target = findClip(draft, clip.id)
              if (target) target.volume = Number(event.target.value)
            })
          }
          aria-label="Clip volume"
        />
      </Field>

      <div className="flex items-center gap-2 border-t border-separator pt-3">
        <Button
          size="sm"
          icon={<Scissors size={12} aria-hidden />}
          onClick={actions.splitAtPlayhead}
          title={`Split at playhead (${shortcutLabel('Mod+B')})`}
        >
          Split
        </Button>
        <Button
          size="sm"
          variant="ghost"
          icon={<Trash2 size={12} aria-hidden />}
          onClick={actions.deleteSelection}
          className="text-danger hover:bg-[color-mix(in_srgb,var(--danger)_10%,transparent)] hover:text-danger"
        >
          Delete
        </Button>
        <span className="ml-auto font-mono text-[10px] text-tertiary">
          {clock(clip.start)} → {clock(clip.start + clip.duration)}
        </span>
      </div>

      <SequenceInfo project={project} />
    </div>
  )
}

function CaptionTextEditor({ caption }: { caption: Caption }): ReactNode {
  const { actions } = useStudio()
  const [text, setText] = useState(caption.text)

  useEffect(() => {
    setText(caption.text)
  }, [caption.id, caption.text])

  const commit = (): void => {
    if (text === caption.text) return
    actions.mutate((draft) => updateCaption(draft, caption.id, { text: text.trim() }))
  }

  return (
    <textarea
      value={text}
      onChange={(event) => setText(event.target.value)}
      onBlur={commit}
      onKeyDown={(event) => {
        if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') event.currentTarget.blur()
      }}
      rows={3}
      aria-label="Caption text"
      className="w-full resize-none select-text rounded-[7px] border border-separator bg-surface px-2.5 py-2 text-[12px] leading-relaxed text-label focus:border-accent/60 focus:outline-none"
    />
  )
}

function CaptionsPanel(): ReactNode {
  const { state, actions } = useStudio()
  const project = state.project
  if (!project) return null
  const selected = project.captions.find((caption) => caption.id === state.selectedCaptionId)

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <div className="text-[12px] font-semibold text-label">Burn-in captions</div>
        <Toggle
          label="Show captions"
          checked={project.settings.captionsEnabled}
          onChange={(checked) =>
            actions.mutate((draft) => {
              draft.settings.captionsEnabled = checked
            })
          }
        />
      </div>

      <Field label="Style">
        <Segmented
          label="Caption style"
          value={project.settings.captionPreset}
          onChange={(value) =>
            actions.mutate((draft) => {
              draft.settings.captionPreset = value
            })
          }
          options={[
            { value: 'clean', label: 'Clean' },
            { value: 'bold', label: 'Bold' },
            { value: 'minimal', label: 'Minimal' }
          ]}
        />
      </Field>

      {selected ? (
        <div className="flex flex-col gap-2.5 rounded-[8px] border border-separator bg-elevated/50 p-2.5">
          <div className="flex items-center justify-between">
            <span className="font-mono text-[10.5px] text-tertiary">
              {clock(selected.start)} → {clock(selected.end)}
            </span>
            <IconButton
              label="Delete caption"
              size="sm"
              onClick={() => {
                actions.mutate((draft) => deleteCaption(draft, selected.id))
                actions.selectCaption(null)
              }}
            >
              <Trash2 size={12} aria-hidden />
            </IconButton>
          </div>
          <CaptionTextEditor caption={selected} />
          <div className="grid grid-cols-2 gap-2">
            <NumberField
              label="Start"
              value={selected.start}
              onCommit={(value) =>
                actions.mutate((draft) => updateCaption(draft, selected.id, { start: value }))
              }
            />
            <NumberField
              label="End"
              min={0.2}
              value={selected.end}
              onCommit={(value) =>
                actions.mutate((draft) =>
                  updateCaption(draft, selected.id, { end: Math.max(value, selected.start + 0.2) })
                )
              }
            />
          </div>
          <p className="text-[10.5px] leading-relaxed text-tertiary">
            Press ⌘↩ to commit edits. Word timings stay linked for karaoke-style highlighting.
          </p>
        </div>
      ) : (
        <p className="rounded-[8px] border border-dashed border-line px-3 py-3 text-[11px] leading-relaxed text-tertiary">
          Select a caption on the timeline or in the transcript to edit its text and timing.
        </p>
      )}

      <Button
        size="sm"
        icon={<CaptionsIcon size={12} aria-hidden />}
        onClick={actions.addCaptionAtPlayhead}
        className="w-full"
      >
        Add caption at playhead
      </Button>

      {project.captions.length ? (
        <div className="flex flex-col gap-0.5 border-t border-separator pt-2">
          <div className="px-0.5 pb-1 text-[10.5px] font-semibold uppercase tracking-[0.05em] text-tertiary">
            All captions · {project.captions.length}
          </div>
          <div className="max-h-[220px] overflow-y-auto">
            {project.captions.map((caption) => (
              <button
                key={caption.id}
                type="button"
                onClick={() => {
                  actions.seek(caption.start + 0.01)
                  actions.selectCaption(caption.id)
                }}
                className={`flex w-full items-baseline gap-2 rounded-[5px] px-1.5 py-1 text-left ${
                  caption.id === selected?.id ? 'bg-accent-soft text-accent' : 'text-secondary hover:bg-elevated'
                }`}
              >
                <span className="shrink-0 font-mono text-[9.5px] tabular-nums text-tertiary">
                  {clock(caption.start)}
                </span>
                <span className="truncate text-[11px]">{caption.text}</span>
              </button>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  )
}

function CapabilityRow({ label, ready, detail }: { label: string; ready: boolean; detail?: string }): ReactNode {
  return (
    <div className="flex items-center gap-2 py-[3px]">
      <span
        className={`flex h-4 w-4 shrink-0 items-center justify-center rounded-full ${
          ready ? 'bg-[color-mix(in_srgb,var(--success)_16%,transparent)] text-success' : 'bg-elevated text-tertiary'
        }`}
      >
        {ready ? <Check size={9} aria-hidden /> : <X size={9} aria-hidden />}
      </span>
      <span className="text-[11px] text-secondary">{label}</span>
      {detail ? <span className="ml-auto truncate text-[10px] text-tertiary">{detail}</span> : null}
    </div>
  )
}

function AIPanel(): ReactNode {
  const { state, actions, targetAssetId } = useStudio()
  const project = state.project
  const [model, setModel] = useState('small.en')
  const [threshold, setThreshold] = useState(-34)
  const [minSilence, setMinSilence] = useState(0.35)
  const [padding, setPadding] = useState(project?.settings.silencePadding ?? 0.12)
  const [sceneThreshold, setSceneThreshold] = useState(27)
  const busy = state.job?.status === 'running' || state.job?.status === 'queued'
  const asset = project?.assets.find((item) => item.id === targetAssetId)
  const engine = state.engine

  if (!project) return null

  return (
    <div className="flex flex-col gap-4">
      <div className="rounded-[8px] border border-separator bg-elevated/50 px-2.5 py-2">
        <div className="flex items-center gap-1.5 text-[10.5px] text-secondary">
          <Sparkles size={12} aria-hidden className="text-accent" />
          Target
          <span className="ml-auto max-w-[170px] truncate text-[10.5px] font-medium text-label">
            {asset?.name ?? 'No media'}
          </span>
        </div>
      </div>

      <section className="flex flex-col gap-2">
        <Button
          variant="primary"
          icon={<Wand2 size={13} aria-hidden />}
          disabled={!asset || busy}
          loading={busy && state.job?.kind === 'silence'}
          onClick={() =>
            void actions.runJob('silence', undefined, {
              silence: { thresholdDb: threshold, minSilence, padding }
            })
          }
          className="w-full justify-start"
        >
          Remove silence & dead air
        </Button>
        <div className="grid grid-cols-3 gap-2">
          <NumberField label="Noise floor" value={threshold} min={-60} max={-10} suffix="dB" onCommit={setThreshold} />
          <NumberField label="Min gap" value={minSilence} min={0.1} max={2} suffix="s" onCommit={setMinSilence} />
          <NumberField label="Padding" value={padding} min={0} max={0.5} suffix="s" onCommit={setPadding} />
        </div>
        <p className="text-[10.5px] leading-relaxed text-tertiary">
          Measured on the audio envelope, then ripple-cut with the neighbours pulled in. Undo restores the
          original timing.
        </p>
      </section>

      <section className="flex flex-col gap-2 border-t border-separator pt-3">
        <Button
          icon={<Layers size={13} aria-hidden />}
          disabled={!asset || busy}
          onClick={() => void actions.runJob('scenes', undefined, { scenes: { threshold: sceneThreshold } })}
          className="w-full justify-start"
        >
          Detect scenes
        </Button>
        <NumberField
          label="Scene sensitivity"
          value={sceneThreshold}
          min={10}
          max={60}
          suffix="th"
          onCommit={setSceneThreshold}
        />
        <p className="text-[10.5px] leading-relaxed text-tertiary">
          PySceneDetect marks hard cuts on the ruler so you can split at a glance.
        </p>
      </section>

      <section className="flex flex-col gap-2 border-t border-separator pt-3">
        <Button
          icon={<AudioLines size={13} aria-hidden />}
          disabled={!asset || busy}
          onClick={() => void actions.runJob('transcribe', undefined, { transcribe: { model } })}
          className="w-full justify-start"
        >
          Transcribe audio
        </Button>
        <Field label="Whisper model" hint="int8 on CPU">
          <select
            value={model}
            onChange={(event) => setModel(event.target.value)}
            className="h-7 appearance-none rounded-[6px] border border-separator bg-surface px-2 text-[11.5px] text-label focus:border-accent/60 focus:outline-none"
          >
            <option value="tiny.en">tiny.en · fastest</option>
            <option value="base.en">base.en</option>
            <option value="small.en">small.en · balanced</option>
            <option value="medium.en">medium.en · most accurate</option>
            <option value="large-v3">large-v3 · multilingual</option>
          </select>
        </Field>
      </section>

      <section className="rounded-[8px] border border-separator bg-surface px-2.5 py-2">
        <div className="mb-1 flex items-center gap-1.5">
          <Cpu size={12} aria-hidden className="text-tertiary" />
          <span className="text-[10.5px] font-semibold uppercase tracking-[0.05em] text-tertiary">Engine</span>
          <Badge tone={engine?.ready ? 'success' : isDemo ? 'warning' : 'danger'}>{isDemo ? 'preview' : engine?.ready ? 'ready' : 'offline'}</Badge>
        </div>
        <CapabilityRow label="FFmpeg" ready={Boolean(engine?.ffmpeg)} detail={engine?.ffprobe ? 'ffprobe' : undefined} />
        <CapabilityRow label="faster-whisper" ready={Boolean(engine?.packages.fasterWhisper)} />
        <CapabilityRow label="PySceneDetect" ready={Boolean(engine?.packages.scenedetect)} />
        <CapabilityRow label="Compute" ready={engine?.device !== 'unknown'} detail={engine?.device?.toUpperCase()} />
        <p className="mt-1.5 border-t border-separator pt-1.5 text-[10px] leading-relaxed text-tertiary">
          {isDemo
            ? 'Preview mode simulates analysis. Desktop runs these tools locally.'
            : engine?.message}
        </p>
      </section>
    </div>
  )
}

export function Inspector(): ReactNode {
  const { state, actions } = useStudio()
  const tabs = [
    { value: 'clip' as const, label: 'Clip', icon: <Layers size={11} aria-hidden /> },
    { value: 'captions' as const, label: 'Captions', icon: <CaptionsIcon size={11} aria-hidden /> },
    { value: 'ai' as const, label: 'AI', icon: <Sparkles size={11} aria-hidden /> }
  ]

  return (
    <aside className="flex w-[292px] shrink-0 flex-col border-l border-separator bg-surface/60">
      <div className="flex h-10 shrink-0 items-center border-b border-separator px-2">
        <Segmented
          label="Inspector"
          value={state.inspectorTab}
          onChange={actions.setInspectorTab}
          options={tabs}
        />
      </div>
      <div className="flex-1 overflow-y-auto p-3">
        {state.inspectorTab === 'clip' ? <ClipPanel /> : null}
        {state.inspectorTab === 'captions' ? <CaptionsPanel /> : null}
        {state.inspectorTab === 'ai' ? <AIPanel /> : null}
      </div>
      <footer className="flex h-7 shrink-0 items-center justify-between border-t border-separator px-2.5 text-[10px] text-tertiary">
        <span>{state.dirty ? 'Unsaved changes' : 'All changes saved'}</span>
        <span className="font-mono">{shortcutLabel('Mod+S')}</span>
      </footer>
    </aside>
  )
}
