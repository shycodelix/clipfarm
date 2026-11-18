import { Pause, Play, Volume2, VolumeX, Captions as CaptionsIcon } from 'lucide-react'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import type { Caption } from '@shared/types'
import { api } from '../lib/bridge'
import { gradientFor, timecode } from '../lib/format'
import { activeClipAt, captionAt, totalDuration } from '../lib/timeline'
import { useStudio } from '../store'
import { Badge, EmptyState, IconButton } from './primitives'

function CaptionOverlay({ caption, preset }: { caption: Caption; preset: string }): ReactNode {
  const styles =
    preset === 'bold'
      ? 'text-[17px] font-semibold leading-snug'
      : preset === 'minimal'
        ? 'text-[12.5px] font-medium leading-snug'
        : 'text-[15px] font-medium leading-snug'
  return (
    <div className="pointer-events-none absolute inset-x-6 bottom-[9%] flex justify-center">
      <span
        className={`${styles} max-w-[86%] rounded-[6px] bg-black/45 px-2.5 py-1 text-center text-white shadow-[0_1px_3px_rgba(0,0,0,0.5)] backdrop-blur-[2px]`}
        style={{ textShadow: '0 1px 2px rgba(0,0,0,0.65)' }}
      >
        {caption.text}
      </span>
    </div>
  )
}

export function Preview(): ReactNode {
  const { state, actions } = useStudio()
  const project = state.project
  const [muted, setMuted] = useState(false)
  const videoRef = useRef<HTMLVideoElement | null>(null)

  const clip = project ? activeClipAt(project, state.playhead) : undefined
  const asset = project && clip ? project.assets.find((item) => item.id === clip.assetId) : undefined
  const caption = project ? captionAt(project, state.playhead) : undefined
  const duration = totalDuration(project)

  useEffect(() => {
    const video = videoRef.current
    if (!video || !clip) return
    const target = clip.inPoint + (state.playhead - clip.start) * clip.speed
    if (Number.isFinite(target) && Math.abs(video.currentTime - target) > 0.3) {
      try {
        video.currentTime = Math.max(0, target)
      } catch {
        // Seeking can fail while metadata is still loading; the next tick retries.
      }
    }
    if (state.playing) void video.play().catch(() => undefined)
    else video.pause()
  }, [clip?.id, state.playing, state.playhead, clip])

  const stageBackground = project
    ? `radial-gradient(120% 130% at 20% 0%, ${gradientFor(project.name)[0]}33, transparent 60%), radial-gradient(100% 120% at 90% 100%, ${gradientFor(project.name)[1]}40, transparent 65%)`
    : undefined

  return (
    <section className="flex min-h-0 min-w-0 flex-1 flex-col bg-[var(--bg-content)]">
      <div className="relative flex min-h-0 min-w-0 flex-1 items-center justify-center overflow-hidden p-4">
        <div className="relative flex h-full w-full items-center justify-center overflow-hidden rounded-[10px] bg-[var(--stage)] shadow-[inset_0_0_0_1px_rgba(255,255,255,0.06)]">
          {project && clip && asset ? (
            <>
              <video
                key={clip.id}
                ref={videoRef}
                className="h-full w-full object-contain"
                src={asset.path ? api.mediaUrl(asset.path) : undefined}
                poster={asset.thumbnailPath ? api.mediaUrl(asset.thumbnailPath) : undefined}
                muted={muted}
                playsInline
                preload="metadata"
              />
              {!asset.path ? (
                <div
                  className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center gap-3"
                  style={{ background: stageBackground }}
                >
                  <div
                    className="flex h-16 w-16 items-center justify-center rounded-[14px] text-[19px] font-semibold text-white/90"
                    style={{
                      background: `linear-gradient(150deg, ${gradientFor(asset.name)[0]}, ${gradientFor(asset.name)[1]})`
                    }}
                  >
                    {asset.name.slice(0, 2).toUpperCase()}
                  </div>
                  <div className="max-w-[70%] truncate text-[12.5px] font-medium text-white/85">{asset.name}</div>
                  <div className="text-[10.5px] uppercase tracking-[0.08em] text-white/40">
                    Demo media · {asset.width}×{asset.height}
                  </div>
                </div>
              ) : null}

              <div className="pointer-events-none absolute left-3 top-3 flex items-center gap-1.5">
                <span className="max-w-[300px] truncate rounded-[5px] bg-black/40 px-1.5 py-0.5 text-[10.5px] font-medium text-white/80 backdrop-blur">
                  {asset.name}
                </span>
                {clip.speed !== 1 ? <Badge tone="warning">{clip.speed}×</Badge> : null}
              </div>
              <div className="pointer-events-none absolute right-3 top-3 rounded-[5px] bg-black/35 px-1.5 py-0.5 font-mono text-[10px] text-white/65 backdrop-blur">
                {project.width}×{project.height} · {project.fps}fps
              </div>
              {caption && project.settings.captionsEnabled ? (
                <CaptionOverlay caption={caption} preset={project.settings.captionPreset} />
              ) : null}
            </>
          ) : (
            <div className="flex flex-col items-center gap-3 px-6 text-center">
              {project && project.clips.length === 0 ? (
                <EmptyState
                  icon={<Play size={20} aria-hidden />}
                  title="The timeline is empty"
                  message="Drop a take into the media bin, then press Import or drag it onto the video track."
                  action={
                    <button
                      type="button"
                      onClick={() => void actions.importMedia()}
                      className="rounded-[7px] bg-white/10 px-3 py-1.5 text-[12px] font-medium text-white/90 hover:bg-white/20"
                    >
                      Import media
                    </button>
                  }
                />
              ) : (
                <>
                  <div className="text-[12.5px] font-medium text-white/70">Nothing at this point</div>
                  <p className="max-w-[280px] text-[11.5px] leading-relaxed text-white/40">
                    Move the playhead over a clip to preview it.
                  </p>
                </>
              )}
            </div>
          )}
        </div>
      </div>

      <div className="material-footer flex h-11 shrink-0 items-center gap-1 overflow-hidden border-t border-separator px-3">
        <button
          type="button"
          onClick={actions.togglePlay}
          aria-label={state.playing ? 'Pause' : 'Play'}
          className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-label text-surface transition hover:opacity-85"
          title={state.playing ? 'Pause (Space)' : 'Play (Space)'}
        >
          {state.playing ? <Pause size={14} aria-hidden /> : <Play size={14} aria-hidden className="ml-0.5" />}
        </button>

        <div className="ml-1.5 flex min-w-0 items-baseline gap-1 font-mono text-[11px] tabular-nums">
          <span className="text-label">{timecode(state.playhead, project?.fps ?? 30)}</span>
          <span className="hidden text-tertiary xl:inline">/</span>
          <span className="hidden text-tertiary xl:inline">{timecode(duration, project?.fps ?? 30)}</span>
        </div>

        <div className="ml-auto flex shrink-0 items-center gap-0.5">
          <IconButton
            label={project?.settings.captionsEnabled ? 'Hide captions' : 'Show captions'}
            active={project?.settings.captionsEnabled}
            size="sm"
            onClick={() =>
              actions.mutate((draft) => {
                draft.settings.captionsEnabled = !draft.settings.captionsEnabled
              })
            }
            disabled={!project}
          >
            <CaptionsIcon size={14} aria-hidden />
          </IconButton>
          <IconButton label={muted ? 'Unmute preview' : 'Mute preview'} size="sm" onClick={() => setMuted((value) => !value)}>
            {muted ? <VolumeX size={14} aria-hidden /> : <Volume2 size={14} aria-hidden />}
          </IconButton>
        </div>
      </div>
    </section>
  )
}
