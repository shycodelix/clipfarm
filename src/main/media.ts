import { randomUUID } from 'node:crypto'
import { execFile } from 'node:child_process'
import { promises as fs } from 'node:fs'
import { basename, extname, join } from 'node:path'
import { BrowserWindow, dialog } from 'electron'
import type { MediaAsset, MediaKind } from '../shared/types'
import { engine } from './engine'
import { thumbnailsDirectory } from './projectStore'

const VIDEO_EXTENSIONS = new Set(['.mp4', '.mov', '.mkv', '.webm', '.m4v', '.avi'])
const AUDIO_EXTENSIONS = new Set(['.wav', '.mp3', '.m4a', '.aac', '.flac', '.aiff', '.ogg'])
const IMAGE_EXTENSIONS = new Set(['.png', '.jpg', '.jpeg', '.webp', '.gif', '.heic'])

const MEDIA_EXTENSIONS = [
  ...VIDEO_EXTENSIONS,
  ...AUDIO_EXTENSIONS,
  ...IMAGE_EXTENSIONS
].map((extension) => extension.slice(1))

export function isPlayableMedia(path: string): boolean {
  return MEDIA_EXTENSIONS.includes(extname(path).slice(1).toLowerCase())
}

function kindFor(path: string): MediaKind {
  const extension = extname(path).toLowerCase()
  if (VIDEO_EXTENSIONS.has(extension)) return 'video'
  if (AUDIO_EXTENSIONS.has(extension)) return 'audio'
  if (IMAGE_EXTENSIONS.has(extension)) return 'image'
  return 'video'
}

interface ProbeResult {
  duration?: number
  width?: number
  height?: number
  fps?: number
  hasAudio?: boolean
}

async function probe(path: string, ffmpegPath: string | null): Promise<ProbeResult> {
  try {
    return await engine.request<ProbeResult>('probe', { path }, { timeoutMs: 45_000 })
  } catch {
    // Without a working engine we can still import the file and let the user
    // analyse it later; we just cannot fill in duration and dimensions yet.
    void ffmpegPath
    return {}
  }
}

function extractThumbnail(path: string, ffmpegPath: string | null, assetId: string): Promise<string | undefined> {
  if (!ffmpegPath || !isPlayableMedia(path)) return Promise.resolve(undefined)
  const target = join(thumbnailsDirectory(), `${assetId}.jpg`)
  return new Promise((resolve) => {
    execFile(
      ffmpegPath,
      [
        '-hide_banner',
        '-loglevel',
        'error',
        '-y',
        '-ss',
        '1',
        '-i',
        path,
        '-frames:v',
        '1',
        '-vf',
        'scale=480:-2',
        '-q:v',
        '6',
        target
      ],
      { timeout: 30_000 },
      (error) => resolve(error ? undefined : target)
    )
  })
}

export async function importPaths(paths: string[], ffmpegPath: string | null): Promise<MediaAsset[]> {
  const unique = [...new Set(paths.filter((path) => isPlayableMedia(path)))]
  const assets = await Promise.all(
    unique.map(async (path) => {
      const id = randomUUID()
      const [metadata, thumbnailPath] = await Promise.all([
        probe(path, ffmpegPath),
        extractThumbnail(path, ffmpegPath, id)
      ])
      return {
        id,
        name: basename(path),
        path,
        kind: kindFor(path),
        duration: metadata.duration ?? 0,
        width: metadata.width ?? 0,
        height: metadata.height ?? 0,
        fps: metadata.fps ?? 0,
        hasAudio: metadata.hasAudio ?? kindFor(path) === 'audio',
        thumbnailPath
      } satisfies MediaAsset
    })
  )
  return assets
}

export async function chooseMedia(window: BrowserWindow | null): Promise<string[]> {
  const options: Electron.OpenDialogOptions = {
    title: 'Import media',
    buttonLabel: 'Import',
    properties: ['openFile', 'multiSelections'],
    filters: [
      { name: 'Video, audio and images', extensions: MEDIA_EXTENSIONS },
      { name: 'All files', extensions: ['*'] }
    ]
  }
  const result = window ? await dialog.showOpenDialog(window, options) : await dialog.showOpenDialog(options)
  return result.canceled ? [] : result.filePaths
}

export async function ensureThumbnailDirectory(): Promise<void> {
  await fs.mkdir(thumbnailsDirectory(), { recursive: true })
}
