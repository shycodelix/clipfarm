import type { Caption, Clip, Project, Scene, Silence, Track } from '@shared/types'
import { totalDuration } from './timeline'

const EPSILON = 1e-6

export function newId(): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) return crypto.randomUUID()
  return `id-${Date.now()}-${Math.random().toString(16).slice(2)}`
}

const round = (value: number): number => Math.round(value * 1000) / 1000

export function videoTrack(project: Project): Track {
  return project.tracks.find((track) => track.kind === 'video') ?? project.tracks[0]
}

export function findClip(project: Project, id: string | null): Clip | undefined {
  return id ? project.clips.find((clip) => clip.id === id) : undefined
}

export function moveClip(
  project: Project,
  clipId: string,
  start: number,
  trackId?: string
): void {
  const clip = project.clips.find((item) => item.id === clipId)
  if (!clip) return
  clip.start = round(Math.max(0, start))
  if (trackId && project.tracks.some((track) => track.id === trackId) && trackId !== 'cc') {
    clip.trackId = trackId
  }
}

export function trimClip(
  project: Project,
  clipId: string,
  edge: 'start' | 'end',
  time: number
): void {
  const clip = project.clips.find((item) => item.id === clipId)
  if (!clip) return
  if (edge === 'start') {
    const maxStart = clip.start + clip.duration - 0.1
    const next = Math.min(Math.max(0, time), maxStart)
    const delta = next - clip.start
    clip.inPoint = round(Math.max(0, clip.inPoint + delta * clip.speed))
    clip.start = round(next)
    clip.duration = round(clip.duration - delta)
  } else {
    const minimumEnd = clip.start + 0.1
    const maximumEnd = clip.inPoint + (clip.duration * clip.speed) > 0 ? clip.start + 3600 : Infinity
    const next = Math.min(Math.max(time, minimumEnd), maximumEnd)
    clip.duration = round(next - clip.start)
  }
}

export function splitClipAt(project: Project, clipId: string, time: number): Clip | null {
  const clip = project.clips.find((item) => item.id === clipId)
  if (!clip) return null
  const offset = time - clip.start
  if (offset <= EPSILON || offset >= clip.duration - EPSILON) return null
  const right: Clip = {
    ...clip,
    id: newId(),
    start: round(time),
    duration: round(clip.duration - offset),
    inPoint: round(clip.inPoint + offset * clip.speed)
  }
  clip.duration = round(offset)
  project.clips.push(right)
  return right
}

export function deleteClip(project: Project, clipId: string): void {
  project.clips = project.clips.filter((clip) => clip.id !== clipId)
}

export function placeAsset(
  project: Project,
  assetId: string,
  trackId: string,
  at: number
): Clip | null {
  const asset = project.assets.find((item) => item.id === assetId)
  if (!asset) return null
  const duration = asset.duration || 5
  const clip: Clip = {
    id: newId(),
    assetId,
    trackId,
    start: round(Math.max(0, at)),
    duration: round(duration),
    inPoint: 0,
    speed: 1,
    volume: 1,
    label: asset.name
  }
  project.clips.push(clip)
  return clip
}

export function appendAssetToTrack(project: Project, assetId: string, trackId = 'v1'): Clip | null {
  const end = project.clips
    .filter((clip) => clip.trackId === trackId)
    .reduce((max, clip) => Math.max(max, clip.start + clip.duration), 0)
  return placeAsset(project, assetId, trackId, end)
}

export function addCaption(project: Project, start: number, end: number, text: string): Caption {
  const caption: Caption = {
    id: newId(),
    start: round(Math.max(0, start)),
    end: round(Math.max(start + 0.2, end)),
    text: text.trim(),
    words: []
  }
  project.captions.push(caption)
  project.captions.sort((a, b) => a.start - b.start)
  return caption
}

export function updateCaption(project: Project, id: string, patch: Partial<Caption>): void {
  const caption = project.captions.find((item) => item.id === id)
  if (!caption) return
  Object.assign(caption, patch)
  project.captions.sort((a, b) => a.start - b.start)
}

export function deleteCaption(project: Project, id: string): void {
  project.captions = project.captions.filter((caption) => caption.id !== id)
}

export function applyScenes(project: Project, scenes: Scene[], assetId: string): void {
  project.scenes = [
    ...project.scenes.filter((scene) => scene.assetId && scene.assetId !== assetId),
    ...scenes.map((scene, index) => ({ ...scene, index, assetId }))
  ]
}

export function replaceCaptions(project: Project, captions: Caption[]): void {
  const incoming = captions.map((caption) => ({
    ...caption,
    id: caption.id || newId(),
    words: (caption.words ?? []).map((word) => ({ ...word }))
  }))
  project.captions = incoming.sort((a, b) => a.start - b.start)
}

function mergeIntervals(intervals: Silence[]): Silence[] {
  const ordered = intervals
    .filter((interval) => interval.end > interval.start)
    .sort((a, b) => a.start - b.start)
  const merged: Silence[] = []
  for (const interval of ordered) {
    const last = merged[merged.length - 1]
    if (last && interval.start <= last.end + 0.001) {
      last.end = Math.max(last.end, interval.end)
    } else {
      merged.push({ start: Math.max(0, interval.start), end: interval.end })
    }
  }
  return merged
}

function subtract(start: number, end: number, cuts: Silence[]): Array<[number, number]> {
  let remaining: Array<[number, number]> = [[start, end]]
  for (const cut of cuts) {
    const next: Array<[number, number]> = []
    for (const [segmentStart, segmentEnd] of remaining) {
      if (cut.end <= segmentStart || cut.start >= segmentEnd) {
        next.push([segmentStart, segmentEnd])
        continue
      }
      if (cut.start > segmentStart) next.push([segmentStart, cut.start])
      if (cut.end < segmentEnd) next.push([cut.end, segmentEnd])
    }
    remaining = next
  }
  return remaining.filter(([a, b]) => b - a > 0.05)
}

/** Port of the engine's ripple cut: remove dead air, keep the cut points calm. */
export function removeSilences(
  project: Project,
  silences: Silence[],
  assetId: string
): { removed: number; cuts: number } {
  const merged = mergeIntervals(silences)
  if (!merged.length) return { removed: 0, cuts: 0 }

  const targets = project.clips
    .filter((clip) => clip.assetId === assetId && clip.trackId !== 'cc')
    .sort((a, b) => a.start - b.start)

  let removedTotal = 0
  let cutCount = 0
  const timelineCuts: Silence[] = []

  for (const clip of targets) {
    const speed = clip.speed || 1
    const start = clip.start
    const end = clip.start + clip.duration
    const cuts: Silence[] = []
    for (const silence of merged) {
      const cutStart = Math.max(start, start + (silence.start - clip.inPoint) / speed)
      const cutEnd = Math.min(end, start + (silence.end - clip.inPoint) / speed)
      if (cutEnd - cutStart > 0.05) cuts.push({ start: cutStart, end: cutEnd })
    }
    if (!cuts.length) continue

    const removed = cuts.reduce((sum, cut) => sum + (cut.end - cut.start), 0)
    const keep = subtract(start, end, cuts)
    timelineCuts.push(...cuts)
    cutCount += cuts.length

    project.clips = project.clips.filter((item) => item.id !== clip.id)

    if (!keep.length) {
      removedTotal += removed
      for (const other of project.clips) {
        if (other.trackId === clip.trackId && other.start >= end - EPSILON) {
          other.start = round(other.start - removed)
        }
      }
      continue
    }

    let cursor = start
    keep.forEach(([keepStart, keepEnd], index) => {
      const fragment: Clip = {
        ...clip,
        id: index === 0 ? clip.id : newId(),
        start: round(cursor),
        duration: round(keepEnd - keepStart),
        inPoint: round(clip.inPoint + (keepStart - start) * speed)
      }
      cursor += keepEnd - keepStart
      project.clips.push(fragment)
    })

    removedTotal += removed
    for (const other of project.clips) {
      if (other.id === clip.id) continue
      if (other.trackId === clip.trackId && other.start >= end - EPSILON) {
        other.start = round(other.start - removed)
      }
    }
  }

  project.clips.sort((a, b) => a.trackId.localeCompare(b.trackId) || a.start - b.start)
  project.silences = []
  if (timelineCuts.length) compactCaptions(project, mergeIntervals(timelineCuts))
  return { removed: round(removedTotal), cuts: cutCount }
}

function compactCaptions(project: Project, cuts: Silence[]): void {
  const remap = (value: number): number | null => {
    let shift = 0
    for (const cut of cuts) {
      if (value >= cut.end) shift += cut.end - cut.start
      else if (value > cut.start) return null
      else break
    }
    return value - shift
  }

  project.captions = project.captions.flatMap((caption) => {
    const start = remap(caption.start)
    const end = remap(caption.end)
    if (start === null || end === null || end - start <= 0.05) return []
    return [{ ...caption, start: round(start), end: round(end) }]
  })
}

export function projectEnd(project: Project): number {
  return totalDuration(project)
}
