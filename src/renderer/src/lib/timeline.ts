import type { Caption, Clip, Project, Track } from '@shared/types'

export const PX_PER_SECOND_MIN = 8
export const PX_PER_SECOND_MAX = 240

export function totalDuration(project: Project | null): number {
  if (!project) return 0
  const clipsEnd = project.clips.reduce((max, clip) => Math.max(max, clip.start + clip.duration), 0)
  const captionsEnd = project.captions.reduce((max, caption) => Math.max(max, caption.end), 0)
  return Math.max(clipsEnd, captionsEnd, 0)
}

export function clipsOnTrack(project: Project, trackId: string): Clip[] {
  return project.clips
    .filter((clip) => clip.trackId === trackId)
    .sort((a, b) => a.start - b.start)
}

export function trackFor(project: Project, trackId: string): Track | undefined {
  return project.tracks.find((track) => track.id === trackId)
}

export function videoTrack(project: Project): Track {
  return project.tracks.find((track) => track.kind === 'video') ?? project.tracks[0]
}

export function captionAt(project: Project, time: number): Caption | undefined {
  return project.captions.find((caption) => time >= caption.start && time <= caption.end)
}

export function activeClipAt(project: Project, time: number): Clip | undefined {
  const track = videoTrack(project)
  if (!track) return undefined
  return clipsOnTrack(project, track.id).find(
    (clip) => time >= clip.start && time < clip.start + clip.duration
  )
}

/** Snap a candidate time to nearby clip edges, scenes and the playhead. */
export function snapTime(
  time: number,
  project: Project,
  options: { playhead?: number; thresholdPx?: number; pxPerSecond: number; excludeClipId?: string }
): number {
  const threshold = (options.thresholdPx ?? 7) / options.pxPerSecond
  const candidates: number[] = [0, totalDuration(project)]
  if (options.playhead !== undefined) candidates.push(options.playhead)
  for (const clip of project.clips) {
    if (clip.id === options.excludeClipId) continue
    candidates.push(clip.start, clip.start + clip.duration)
  }
  for (const caption of project.captions) {
    if (caption.id === options.excludeClipId) continue
    candidates.push(caption.start, caption.end)
  }
  for (const scene of project.scenes) candidates.push(scene.start, scene.end)

  let best = time
  let bestDistance = threshold
  for (const candidate of candidates) {
    const distance = Math.abs(candidate - time)
    if (distance < bestDistance) {
      best = candidate
      bestDistance = distance
    }
  }
  return Math.max(0, best)
}

export function clipSourceRange(clip: Clip): [number, number] {
  return [clip.inPoint, clip.inPoint + clip.duration * clip.speed]
}

/** Map asset-source scene boundaries onto timeline positions. */
export function sceneMarkers(project: Project): number[] {
  const markers = new Set<number>()
  for (const clip of project.clips) {
    for (const scene of project.scenes) {
      if (scene.assetId !== clip.assetId) continue
      if (scene.start > clip.inPoint && scene.start < clip.inPoint + clip.duration * clip.speed) {
        markers.add(clip.start + (scene.start - clip.inPoint) / clip.speed)
      }
    }
  }
  return [...markers].sort((a, b) => a - b)
}

export const MIN_CLIP_DURATION = 0.2
