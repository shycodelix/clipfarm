import {
  Captions,
  Film,
  Image as ImageIcon,
  Music,
  Plus,
  Search,
  Sparkles,
  Upload,
  X
} from 'lucide-react'
import { useMemo, useState, type DragEvent, type ReactNode } from 'react'
import type { MediaAsset } from '@shared/types'
import { api } from '../lib/bridge'
import { clock, gradientFor } from '../lib/format'
import { useStudio } from '../store'
import { Badge, Button, EmptyState, IconButton, Segmented, TextInput } from './primitives'

function kindIcon(asset: MediaAsset): ReactNode {
  if (asset.kind === 'audio') return <Music size={12} aria-hidden />
  if (asset.kind === 'image') return <ImageIcon size={12} aria-hidden />
  return <Film size={12} aria-hidden />
}

function AssetRow({ asset }: { asset: MediaAsset }): ReactNode {
  const { state, actions } = useStudio()
  const [from, to] = gradientFor(asset.name)
  const thumbnail = asset.thumbnailPath ? api.mediaUrl(asset.thumbnailPath) : ''

  return (
    <div
      role="button"
      tabIndex={0}
      draggable
      onDragStart={(event) => {
        event.dataTransfer.setData('application/clipfarm-asset', asset.id)
        event.dataTransfer.effectAllowed = 'copy'
      }}
      onClick={() => actions.addAssetToTimeline(asset)}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault()
          actions.addAssetToTimeline(asset)
        }
      }}
      title={`Add ${asset.name} to the timeline`}
      className="group/asset flex cursor-grab items-center gap-2.5 rounded-[7px] px-1.5 py-1.5 text-left transition-colors hover:bg-elevated active:cursor-grabbing"
    >
      <div
        className="relative flex h-9 w-14 shrink-0 items-center justify-center overflow-hidden rounded-[5px] text-white/70"
        style={{ background: thumbnail ? undefined : `linear-gradient(140deg, ${from}, ${to})` }}
      >
        {thumbnail ? (
          <img src={thumbnail} alt="" className="h-full w-full object-cover" draggable={false} />
        ) : (
          kindIcon(asset)
        )}
        <span className="absolute bottom-0 right-0 rounded-tl-[3px] bg-black/50 px-1 font-mono text-[9px] leading-[13px] text-white/85">
          {clock(asset.duration)}
        </span>
      </div>
      <div className="min-w-0 flex-1">
        <div className="truncate text-[11.5px] font-medium text-label">{asset.name}</div>
        <div className="mt-0.5 flex items-center gap-1.5 text-[10px] text-tertiary">
          <span className="uppercase tracking-[0.04em]">{asset.kind}</span>
          {asset.width ? (
            <>
              <span aria-hidden>·</span>
              <span>
                {asset.width}×{asset.height}
              </span>
            </>
          ) : null}
          {asset.kind === 'video' && state.project?.scenes.some((scene) => scene.assetId === asset.id) ? (
            <Badge tone="accent">scenes</Badge>
          ) : null}
        </div>
      </div>
      <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-[5px] text-tertiary opacity-0 transition group-hover/asset:opacity-100">
        <Plus size={13} aria-hidden />
      </span>
    </div>
  )
}

function MediaBin(): ReactNode {
  const { state, actions } = useStudio()
  const [dragging, setDragging] = useState(false)
  const [query, setQuery] = useState('')
  const assets = useMemo(() => {
    const list = state.project?.assets ?? []
    if (!query.trim()) return list
    return list.filter((asset) => asset.name.toLowerCase().includes(query.toLowerCase()))
  }, [state.project?.assets, query])

  const onDrop = (event: DragEvent<HTMLDivElement>): void => {
    event.preventDefault()
    setDragging(false)
    const files = Array.from(event.dataTransfer.files ?? [])
    if (files.length) void actions.dropFiles(files)
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-1.5 px-2 pb-2">
        <TextInput
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Filter media"
          aria-label="Filter media"
          className="h-6 bg-transparent px-1.5 text-[11px]"
        />
        <IconButton label="Import media" shortcut="Mod+I" size="sm" onClick={() => void actions.importMedia()}>
          <Upload size={13} aria-hidden />
        </IconButton>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-1.5">
        {assets.length === 0 ? (
          <EmptyState
            compact
            icon={<Film size={17} aria-hidden />}
            title="No media yet"
            message="Drag a video or audio file here, or import from disk."
            action={
              <Button size="sm" onClick={() => void actions.importMedia()} icon={<Upload size={12} aria-hidden />}>
                Import
              </Button>
            }
          />
        ) : (
          <div className="flex flex-col gap-0.5">
            {assets.map((asset) => (
              <AssetRow key={asset.id} asset={asset} />
            ))}
          </div>
        )}
      </div>

      <div
        onDragOver={(event) => {
          event.preventDefault()
          setDragging(true)
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={onDrop}
        className={`m-2 flex h-[54px] shrink-0 items-center justify-center rounded-[8px] border border-dashed text-[10.5px] transition-colors ${
          dragging
            ? 'border-accent bg-accent-soft text-accent'
            : 'border-line text-tertiary hover:border-accent/50 hover:text-secondary'
        }`}
      >
        Drop media to import
      </div>
    </div>
  )
}

function TranscriptPanel(): ReactNode {
  const { state, actions } = useStudio()
  const [query, setQuery] = useState('')
  const project = state.project
  const captions = useMemo(() => {
    const list = project?.captions ?? []
    if (!query.trim()) return list
    return list.filter((caption) => caption.text.toLowerCase().includes(query.toLowerCase()))
  }, [project?.captions, query])

  if (!project) return null

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="px-2 pb-2">
        <div className="relative">
          <Search
            size={12}
            aria-hidden
            className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 text-tertiary"
          />
          <TextInput
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search transcript"
            aria-label="Search transcript"
            className="h-7 pl-6 pr-6 text-[11px]"
          />
          {query ? (
            <button
              type="button"
              aria-label="Clear search"
              onClick={() => setQuery('')}
              className="absolute right-1 top-1/2 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded text-tertiary hover:text-label"
            >
              <X size={11} aria-hidden />
            </button>
          ) : null}
        </div>
        <div className="mt-1.5 flex items-center justify-between px-0.5 text-[10.5px] text-tertiary">
          <span>{project.captions.length} captions</span>
          <span>word-level timing</span>
        </div>
      </div>

      {project.captions.length === 0 ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <EmptyState
            compact
            icon={<Captions size={17} aria-hidden />}
            title="No transcript yet"
            message="Transcribe the selected clip to get captions with per-word timestamps."
            action={
              <Button
                size="sm"
                variant="primary"
                icon={<Sparkles size={12} aria-hidden />}
                onClick={() => void actions.runJob('transcribe')}
              >
                Transcribe
              </Button>
            }
          />
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto px-1.5 pb-2">
          {captions.map((caption) => {
            const selected = state.selectedCaptionId === caption.id
            const active = state.playhead >= caption.start && state.playhead <= caption.end
            return (
              <button
                key={caption.id}
                type="button"
                onClick={() => {
                  actions.seek(caption.start + 0.01)
                  actions.selectCaption(caption.id)
                }}
                className={`mb-0.5 flex w-full items-start gap-2 rounded-[6px] px-1.5 py-1.5 text-left transition-colors ${
                  selected ? 'bg-accent-soft' : active ? 'bg-elevated' : 'hover:bg-elevated'
                }`}
              >
                <span
                  className={`mt-[1px] shrink-0 font-mono text-[10px] tabular-nums ${
                    selected ? 'text-accent' : 'text-tertiary'
                  }`}
                >
                  {clock(caption.start)}
                </span>
                <span
                  className={`text-[11.5px] leading-[1.5] ${
                    active ? 'text-label' : 'text-secondary'
                  }`}
                >
                  {caption.text}
                </span>
              </button>
            )
          })}
          {captions.length === 0 ? (
            <p className="px-2 py-4 text-center text-[11px] text-tertiary">No captions match that search.</p>
          ) : null}
        </div>
      )}
    </div>
  )
}

export function LeftPanel(): ReactNode {
  const { state, actions } = useStudio()
  const project = state.project
  const totalAssets = project?.assets.length ?? 0

  return (
    <aside className="flex w-[252px] shrink-0 flex-col border-r border-separator bg-surface/60">
      <div className="flex h-10 shrink-0 items-center justify-between gap-2 border-b border-separator px-2">
        <Segmented
          label="Media and transcript"
          value={state.leftPanel}
          onChange={actions.setLeftPanel}
          options={[
            { value: 'media', label: 'Media', icon: <Film size={11} aria-hidden /> },
            { value: 'transcript', label: 'Transcript', icon: <Captions size={11} aria-hidden /> }
          ]}
        />
        <span className="pr-1 text-[10.5px] tabular-nums text-tertiary">
          {state.leftPanel === 'media' ? `${totalAssets}` : `${project?.captions.length ?? 0}`}
        </span>
      </div>
      {state.leftPanel === 'media' ? <MediaBin /> : <TranscriptPanel />}
      <footer className="flex h-7 shrink-0 items-center justify-between border-t border-separator px-2.5 text-[10px] text-tertiary">
        <span>{project?.assets.length ?? 0} assets in project</span>
        <span>Click or drag to add</span>
      </footer>
    </aside>
  )
}
