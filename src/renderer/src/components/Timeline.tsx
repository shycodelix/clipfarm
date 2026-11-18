import {
  Captions as CaptionsIcon,
  Minus,
  Plus,
  Scissors,
  Trash2,
  ZoomIn
} from 'lucide-react'
import {
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type ReactNode
} from 'react'
import type { Clip, Project, Track } from '@shared/types'
import { moveClip, placeAsset, trimClip } from '../lib/edits'
import { clamp, clock, shortcut as shortcutLabel } from '../lib/format'
import {
  clipsOnTrack,
  sceneMarkers,
  snapTime,
  totalDuration,
  videoTrack
} from '../lib/timeline'
import { useStudio } from '../store'
import { Button, EmptyState, IconButton } from './primitives'

const HEADER_WIDTH = 112
const RULER_HEIGHT = 24
const TRAILING_SPACE = 140

type DragMode = 'move' | 'trim-start' | 'trim-end'

interface DragState {
  mode: DragMode
  clipId: string
  startX: number
  origin: Clip
  snapshot: Project
}

function waveform(clip: Clip): number[] {
  const seed = clip.id.split('').reduce((total, char) => total + char.charCodeAt(0), 7)
  return Array.from({ length: 34 }, (_, index) => {
    const value = Math.sin((index + seed) * 1.73) * Math.cos((index + seed) * 0.41)
    return 3 + Math.abs(value) * 11
  })
}

function ClipBlock({
  clip,
  track,
  project,
  zoom,
  selected,
  onBegin,
  onMove,
  onEnd
}: {
  clip: Clip
  track: Track
  project: Project
  zoom: number
  selected: boolean
  onBegin: (event: ReactPointerEvent, clip: Clip, mode: DragMode) => void
  onMove: (event: ReactPointerEvent) => void
  onEnd: () => void
}): ReactNode {
  const asset = project.assets.find((item) => item.id === clip.assetId)
  const isOverlay = track.kind === 'overlay'
  // Calm, track-keyed colors keep the timeline readable; asset gradients stay in the bins.
  const background = isOverlay
    ? 'linear-gradient(180deg, #6a59bd, #51419b)'
    : 'linear-gradient(180deg, #4a7aae, #355d87)'
  const width = Math.max(clip.duration * zoom, 22)
  const bars = waveform(clip)

  const silenceSegments = project.silences
    .map((silence) => {
      const start = clip.start + (silence.start - clip.inPoint) / clip.speed
      const end = clip.start + (silence.end - clip.inPoint) / clip.speed
      const left = Math.max(start, clip.start)
      const right = Math.min(end, clip.start + clip.duration)
      if (right - left <= 0.03) return null
      return { left: (left - clip.start) * zoom, width: (right - left) * zoom }
    })
    .filter((segment): segment is { left: number; width: number } => segment !== null)

  return (
    <div
      role="button"
      tabIndex={0}
      aria-label={`${asset?.name ?? 'Clip'} at ${clock(clip.start)}`}
      onPointerDown={(event) => onBegin(event, clip, 'move')}
      onPointerMove={onMove}
      onPointerUp={onEnd}
      onPointerCancel={onEnd}
      onKeyDown={(event) => {
        if (event.key === 'Backspace' || event.key === 'Delete') {
          // Selection + delete is handled globally; keep Enter as a seek affordance.
        }
      }}
      className={`clip-body group/clip absolute top-[5px] bottom-[5px] overflow-hidden rounded-[5px] ${
        selected ? 'ring-2 ring-white/90 ring-offset-1 ring-offset-transparent' : 'ring-1 ring-black/20'
      }`}
      style={{
        left: clip.start * zoom,
        width,
        background,
        cursor: 'grab'
      }}
      title={`${asset?.name ?? 'Clip'} · ${clock(clip.duration)} · in ${clock(clip.inPoint)}`}
    >
      {silenceSegments.map((segment, index) => (
        <span
          key={index}
          aria-hidden
          className="pointer-events-none absolute inset-y-0"
          style={{
            left: segment.left,
            width: segment.width,
            background:
              'repeating-linear-gradient(115deg, rgba(8,10,14,0.22) 0 5px, rgba(8,10,14,0.07) 5px 10px)'
          }}
        />
      ))}

      <div className="pointer-events-none relative z-10 flex items-center gap-1 px-1.5 pt-1">
        <span className="truncate text-[10.5px] font-medium text-white/95">
          {clip.label ?? asset?.name ?? 'Clip'}
        </span>
        {clip.speed !== 1 ? (
          <span className="shrink-0 rounded-[3px] bg-black/30 px-1 text-[9px] text-white/85">{clip.speed}×</span>
        ) : null}
      </div>

      {width > 46 ? (
        <div className="pointer-events-none absolute inset-x-1 bottom-1 flex h-4 items-end gap-[2px] opacity-55">
          {bars.slice(0, Math.min(bars.length, Math.floor(width / 4))).map((height, index) => (
            <span
              key={index}
              className="w-[2px] shrink-0 rounded-full bg-white/80"
              style={{ height }}
            />
          ))}
        </div>
      ) : null}

      <div
        role="presentation"
        onPointerDown={(event) => onBegin(event, clip, 'trim-start')}
        className="absolute inset-y-0 left-0 w-[6px] cursor-col-resize bg-white/0 transition-colors hover:bg-white/30"
      />
      <div
        role="presentation"
        onPointerDown={(event) => onBegin(event, clip, 'trim-end')}
        className="absolute inset-y-0 right-0 w-[6px] cursor-col-resize bg-white/0 transition-colors hover:bg-white/30"
      />
    </div>
  )
}

export function Timeline(): ReactNode {
  const { state, actions } = useStudio()
  const project = state.project
  const scrollRef = useRef<HTMLDivElement | null>(null)
  const rulerRef = useRef<HTMLDivElement | null>(null)
  const dragRef = useRef<DragState | null>(null)
  const [dragging, setDragging] = useState(false)
  const [dropTrack, setDropTrack] = useState<string | null>(null)

  const zoom = state.zoom
  const duration = Math.max(totalDuration(project), 8)
  const contentWidth = duration * zoom + TRAILING_SPACE
  const tracks = project?.tracks ?? []
  const markers = useMemo(() => (project ? sceneMarkers(project) : []), [project])
  const tickStep = useMemo(() => {
    const steps = [0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600]
    return steps.find((step) => step * zoom >= 72) ?? 600
  }, [zoom])
  const ticks = useMemo(() => {
    const count = Math.ceil((duration + TRAILING_SPACE / zoom) / tickStep) + 1
    return Array.from({ length: count }, (_, index) => index * tickStep)
  }, [duration, tickStep, zoom])

  if (!project) {
    return (
      <section className="flex h-[286px] shrink-0 items-center justify-center border-t border-separator bg-surface/70">
        <EmptyState
          compact
          icon={<Scissors size={18} aria-hidden />}
          title="No project open"
          message="Open a project to see its timeline."
        />
      </section>
    )
  }

  const seekFromClientX = (clientX: number): void => {
    const rect = rulerRef.current?.getBoundingClientRect()
    if (!rect) return
    actions.seek(clamp((clientX - rect.left) / zoom, 0, duration))
  }

  const beginDrag = (event: ReactPointerEvent, clip: Clip, mode: DragMode): void => {
    event.stopPropagation()
    event.preventDefault()
    actions.selectClip(clip.id)
    dragRef.current = {
      mode,
      clipId: clip.id,
      startX: event.clientX,
      origin: { ...clip },
      snapshot: structuredClone(project)
    }
    setDragging(true)
    ;(event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId)
  }

  const dragMove = (event: ReactPointerEvent): void => {
    const drag = dragRef.current
    if (!drag) return
    const delta = (event.clientX - drag.startX) / zoom
    const snapOptions = {
      playhead: state.playhead,
      pxPerSecond: zoom,
      excludeClipId: drag.clipId
    }
    if (drag.mode === 'move') {
      const target = snapTime(drag.origin.start + delta, project, snapOptions)
      actions.mutate((draft) => moveClip(draft, drag.clipId, target), true)
    } else if (drag.mode === 'trim-start') {
      const target = snapTime(drag.origin.start + delta, project, snapOptions)
      actions.mutate((draft) => trimClip(draft, drag.clipId, 'start', target), true)
    } else {
      const target = snapTime(drag.origin.start + drag.origin.duration + delta, project, snapOptions)
      actions.mutate((draft) => trimClip(draft, drag.clipId, 'end', target), true)
    }
  }

  const endDrag = (): void => {
    const drag = dragRef.current
    if (!drag) return
    dragRef.current = null
    setDragging(false)
    actions.commitHistory(drag.snapshot)
  }

  const fit = (): void => {
    const width = (scrollRef.current?.clientWidth ?? 900) - HEADER_WIDTH - 32
    actions.setZoom(clamp(width / Math.max(duration, 1), 8, 240))
  }

  const handleDrop = (event: React.DragEvent<HTMLDivElement>, trackId: string): void => {
    event.preventDefault()
    setDropTrack(null)
    if (trackId === 'cc') return
    const assetId = event.dataTransfer.getData('application/clipfarm-asset')
    if (!assetId) return
    const rect = event.currentTarget.getBoundingClientRect()
    const at = snapTime((event.clientX - rect.left) / zoom, project, {
      playhead: state.playhead,
      pxPerSecond: zoom
    })
    actions.mutate((draft) => {
      placeAsset(draft, assetId, trackId, at)
    })
  }

  const playheadLeft = HEADER_WIDTH + state.playhead * zoom

  return (
    <section className="flex min-h-0 shrink-0 flex-col border-t border-separator bg-surface/40" style={{ height: 300 }}>
      <header className="flex h-9 shrink-0 items-center gap-1 border-b border-separator px-2">
        <span className="pl-1 pr-2 text-[11.5px] font-semibold text-label">Timeline</span>
        <span className="rounded-[4px] bg-elevated px-1.5 py-0.5 text-[10px] tabular-nums text-secondary">
          {clock(state.playhead)} · {clock(duration)}
        </span>
        <div className="ml-2 flex items-center gap-0.5 border-l border-separator pl-2">
          <IconButton label="Split at playhead" shortcut="Mod+B" size="sm" onClick={actions.splitAtPlayhead}>
            <Scissors size={13} aria-hidden />
          </IconButton>
          <IconButton label="Add caption" shortcut="Alt+Mod+C" size="sm" onClick={actions.addCaptionAtPlayhead}>
            <CaptionsIcon size={13} aria-hidden />
          </IconButton>
          <IconButton label="Delete selection" size="sm" onClick={actions.deleteSelection}>
            <Trash2 size={13} aria-hidden />
          </IconButton>
        </div>

        <div className="ml-auto flex items-center gap-1.5">
          <span className="text-[10px] text-tertiary">{dragging ? 'Snapping' : 'Snap on'}</span>
          <IconButton label="Zoom out" size="sm" onClick={() => actions.setZoom(zoom / 1.35)}>
            <Minus size={12} aria-hidden />
          </IconButton>
          <input
            type="range"
            min={8}
            max={240}
            step={1}
            value={zoom}
            onChange={(event) => actions.setZoom(Number(event.target.value))}
            aria-label="Timeline zoom"
            className="w-[84px]"
          />
          <IconButton label="Zoom in" size="sm" onClick={() => actions.setZoom(zoom * 1.35)}>
            <Plus size={12} aria-hidden />
          </IconButton>
          <Button size="sm" variant="ghost" icon={<ZoomIn size={12} aria-hidden />} onClick={fit}>
            Fit
          </Button>
        </div>
      </header>

      <div ref={scrollRef} className="relative min-h-0 flex-1 overflow-auto overscroll-contain">
        <div className="relative" style={{ width: HEADER_WIDTH + contentWidth }}>
          <div className="sticky top-0 z-30 flex" style={{ height: RULER_HEIGHT }}>
            <div className="sticky left-0 z-40 shrink-0 border-b border-r border-separator bg-surface" style={{ width: HEADER_WIDTH }} />
            <div
              ref={rulerRef}
              className="timeline-grid relative shrink-0 cursor-ew-resize border-b border-separator bg-surface/70"
              style={{ width: contentWidth }}
              onPointerDown={(event) => {
                event.currentTarget.setPointerCapture(event.pointerId)
                seekFromClientX(event.clientX)
              }}
              onPointerMove={(event) => {
                if (event.buttons === 1) seekFromClientX(event.clientX)
              }}
            >
              {ticks.map((time) => (
                <div key={time} className="absolute inset-y-0" style={{ left: time * zoom }}>
                  <div className="absolute bottom-0 h-1.5 w-px bg-line" />
                  <span className="absolute bottom-[7px] left-1 font-mono text-[9px] tabular-nums text-tertiary">
                    {clock(time)}
                  </span>
                </div>
              ))}
              {markers.map((marker) => (
                <span
                  key={`marker-${marker}`}
                  aria-hidden
                  className="absolute bottom-0 h-0 w-0 border-x-[3px] border-b-[4px] border-x-transparent border-b-[color-mix(in_srgb,var(--accent)_70%,transparent)]"
                  style={{ left: marker * zoom }}
                />
              ))}
            </div>
          </div>

          {tracks.map((track) => {
            const isCaptionTrack = track.kind === 'captions'
            const trackClips = clipsOnTrack(project, track.id)
            return (
              <div key={track.id} className="flex border-b border-separator/70" style={{ height: track.height }}>
                <div
                  className="sticky left-0 z-20 flex shrink-0 flex-col justify-center gap-0.5 border-r border-separator bg-surface px-2"
                  style={{ width: HEADER_WIDTH }}
                >
                  <div className="flex items-center gap-1.5">
                    <span
                      className={`h-1.5 w-1.5 rounded-full ${
                        track.kind === 'video'
                          ? 'bg-accent'
                          : track.kind === 'overlay'
                            ? 'bg-[#7a5af8]'
                            : 'bg-warning'
                      }`}
                    />
                    <span className="text-[11px] font-medium text-label">{track.name}</span>
                  </div>
                  <span className="text-[9.5px] text-tertiary">
                    {isCaptionTrack
                      ? `${project.captions.length} captions`
                      : `${trackClips.length} ${trackClips.length === 1 ? 'clip' : 'clips'}`}
                  </span>
                </div>

                <div
                  onDragOver={(event) => {
                    if (isCaptionTrack) return
                    event.preventDefault()
                    setDropTrack(track.id)
                  }}
                  onDragLeave={() => setDropTrack(null)}
                  onDrop={(event) => handleDrop(event, track.id)}
                  onPointerDown={(event) => {
                    if (event.target === event.currentTarget) {
                      actions.selectClip(null)
                      actions.selectCaption(null)
                    }
                  }}
                  className={`relative shrink-0 ${
                    dropTrack === track.id ? 'bg-accent-soft' : ''
                  }`}
                  style={{ width: contentWidth }}
                >
                  <div
                    aria-hidden
                    className="pointer-events-none absolute inset-x-0 top-1/2 h-px bg-[var(--separator)]"
                  />

                  {isCaptionTrack
                    ? project.captions.map((caption) => (
                        <button
                          key={caption.id}
                          type="button"
                          onPointerDown={(event) => event.stopPropagation()}
                          onClick={() => {
                            actions.seek(caption.start + 0.01)
                            actions.selectCaption(caption.id)
                          }}
                          className={`absolute top-[6px] bottom-[6px] overflow-hidden rounded-[4px] px-1.5 text-left text-[10px] leading-[16px] text-white/95 transition ${
                            state.selectedCaptionId === caption.id
                              ? 'bg-warning ring-2 ring-white/80'
                              : 'bg-[color-mix(in_srgb,var(--warning)_78%,#7a4c00)] hover:brightness-110'
                          }`}
                          style={{
                            left: caption.start * zoom,
                            width: Math.max((caption.end - caption.start) * zoom, 18)
                          }}
                          title={caption.text}
                        >
                          <span className="block truncate">{caption.text}</span>
                        </button>
                      ))
                    : trackClips.map((clip) => (
                        <ClipBlock
                          key={clip.id}
                          clip={clip}
                          track={track}
                          project={project}
                          zoom={zoom}
                          selected={state.selectedClipId === clip.id}
                          onBegin={beginDrag}
                          onMove={dragMove}
                          onEnd={endDrag}
                        />
                      ))}

                  {track.kind === 'video' && trackClips.length === 0 ? (
                    <div className="pointer-events-none absolute inset-0 flex items-center justify-center gap-2 text-[11px] text-tertiary">
                      <Scissors size={12} aria-hidden />
                      Drop media here or press {shortcutLabel('Mod+I')} to import
                    </div>
                  ) : null}
                </div>
              </div>
            )
          })}

          {markers.map((marker) => (
            <span
              key={`line-${marker}`}
              aria-hidden
              className="pointer-events-none absolute bottom-0 z-10 w-px bg-[color-mix(in_srgb,var(--accent)_32%,transparent)]"
              style={{ left: HEADER_WIDTH + marker * zoom, top: RULER_HEIGHT }}
            />
          ))}

          <div
            className="pointer-events-none absolute bottom-0 z-40 w-px bg-accent"
            style={{ left: playheadLeft, top: RULER_HEIGHT - 4 }}
          >
            <span className="absolute -left-[4px] -top-1 h-2 w-2 rotate-45 rounded-[1px] bg-accent" />
          </div>
        </div>
      </div>

      <footer className="flex h-6 shrink-0 items-center justify-between border-t border-separator px-2.5 text-[10px] text-tertiary">
        <span>
          {project.clips.length} clips · {tracks.length} tracks · {videoTrack(project).name}
        </span>
        <span className="flex items-center gap-2">
          <span>Space play</span>
          <span aria-hidden>·</span>
          <span>{shortcutLabel('Mod+B')} split</span>
          <span aria-hidden>·</span>
          <span>{shortcutLabel('Shift+Mod+D')} remove silence</span>
        </span>
      </footer>
    </section>
  )
}
