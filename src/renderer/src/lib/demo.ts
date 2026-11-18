import type {
  Caption,
  ClipfarmApi,
  EngineStatus,
  Job,
  JobResult,
  McpConfig,
  MediaAsset,
  Project,
  RunRequest,
  Scene,
  Silence,
  Word
} from '@shared/types'

/** Only used by the browser preview: JSDoc keeps this file honest about its scope. */
type RunKind = RunRequest['kind']

const now = new Date()
const iso = (offsetMinutes: number): string =>
  new Date(now.getTime() - offsetMinutes * 60_000).toISOString()

function wordsFor(text: string, start: number, end: number): Word[] {
  const tokens = text.split(/\s+/).filter(Boolean)
  const span = (end - start) / Math.max(tokens.length, 1)
  return tokens.map((token, index) => ({
    text: token,
    start: Number((start + index * span).toFixed(3)),
    end: Number((start + (index + 1) * span).toFixed(3)),
    confidence: 0.94
  }))
}

function caption(text: string, start: number, end: number): Caption {
  return { id: `cap-${start}-${text.length}`, start, end, text, words: wordsFor(text, start, end) }
}

function asset(
  id: string,
  name: string,
  duration: number,
  options: Partial<MediaAsset> = {}
): MediaAsset {
  return {
    id,
    name,
    path: '',
    kind: 'video',
    duration,
    width: 1920,
    height: 1080,
    fps: 30,
    hasAudio: true,
    ...options
  }
}

const interview = asset('a-interview', 'Mara Chen — Interview.mp4', 186.4, { fps: 24 })
const broll = asset('a-broll', 'Studio B-Roll.mp4', 44.8, { width: 3840, height: 2160, fps: 24 })
const macro = asset('a-macro', 'Product Macro.mov', 12.6, {
  width: 1920,
  height: 1080,
  fps: 60
})
const roomTone = asset('a-tone', 'Room Tone.wav', 30, {
  kind: 'audio',
  width: 0,
  height: 0,
  fps: 0
})

const studioProject: Project = {
  schemaVersion: 1,
  id: 'demo-studio',
  name: 'Studio Session — Mara Chen',
  createdAt: iso(2600),
  updatedAt: iso(4),
  fps: 30,
  width: 1920,
  height: 1080,
  assets: [interview, broll, macro, roomTone],
  tracks: [
    { id: 'v2', name: 'Overlay', kind: 'overlay', height: 58, muted: false, locked: false },
    { id: 'v1', name: 'Video', kind: 'video', height: 74, muted: false, locked: false },
    { id: 'cc', name: 'Captions', kind: 'captions', height: 46, muted: false, locked: false }
  ],
  clips: [
    { id: 'c1', assetId: interview.id, trackId: 'v1', start: 0, duration: 21.8, inPoint: 4.2, speed: 1, volume: 1 },
    { id: 'c2', assetId: interview.id, trackId: 'v1', start: 21.8, duration: 16.2, inPoint: 30.4, speed: 1, volume: 1 },
    { id: 'c3', assetId: broll.id, trackId: 'v1', start: 38, duration: 9.4, inPoint: 6, speed: 1, volume: 1 },
    { id: 'c4', assetId: interview.id, trackId: 'v1', start: 47.4, duration: 24.8, inPoint: 58.6, speed: 1, volume: 1 },
    { id: 'c5', assetId: macro.id, trackId: 'v2', start: 9.6, duration: 3.6, inPoint: 0.8, speed: 1, volume: 1 },
    { id: 'c6', assetId: broll.id, trackId: 'v2', start: 27, duration: 5.2, inPoint: 18.2, speed: 1, volume: 1 }
  ],
  captions: [
    caption('Everyone says the edit is the easy part.', 0.8, 4.2),
    caption('Then you open a three-hour interview.', 4.6, 8.4),
    caption('Dead air everywhere — half of it is mine.', 10.2, 14.2),
    caption('The fix is not a better microphone.', 15.4, 18.8),
    caption('It is hearing the pause before it happens.', 22.4, 27.2),
    caption('So we built the quiet parts into the tool.', 28.4, 33.6),
    caption('Cut, breathe, and keep the sentence intact.', 47.8, 52.6),
    caption('That is the whole trick, really.', 53.2, 57.4),
    caption('Everything else is just timing.', 58.6, 63.2)
  ],
  scenes: [
    { index: 0, assetId: interview.id, start: 4.2, end: 42.6 },
    { index: 1, assetId: interview.id, start: 42.6, end: 96.8 },
    { index: 2, assetId: interview.id, start: 96.8, end: 186.4 },
    { index: 0, assetId: broll.id, start: 6, end: 20.4 },
    { index: 1, assetId: broll.id, start: 20.4, end: 44.8 }
  ],
  silences: [
    { start: 2.1, end: 3.4 },
    { start: 9.4, end: 11.9 },
    { start: 19.8, end: 21.6 },
    { start: 34.2, end: 36.8 },
    { start: 44.6, end: 47.2 },
    { start: 61.4, end: 63.9 },
    { start: 78.2, end: 82.4 },
    { start: 104.6, end: 107.2 },
    { start: 128.4, end: 131.8 },
    { start: 150.2, end: 153.6 }
  ],
  settings: {
    captionsEnabled: true,
    captionPreset: 'clean',
    silencePadding: 0.12,
    autoRipple: true
  }
}

const teaserProject: Project = {
  schemaVersion: 1,
  id: 'demo-teaser',
  name: 'Q4 Product Teaser',
  createdAt: iso(9200),
  updatedAt: iso(1500),
  fps: 24,
  width: 1920,
  height: 1080,
  assets: [broll, macro],
  tracks: [
    { id: 'v2', name: 'Overlay', kind: 'overlay', height: 58, muted: false, locked: false },
    { id: 'v1', name: 'Video', kind: 'video', height: 74, muted: false, locked: false },
    { id: 'cc', name: 'Captions', kind: 'captions', height: 46, muted: false, locked: false }
  ],
  clips: [
    { id: 't1', assetId: broll.id, trackId: 'v1', start: 0, duration: 6.5, inPoint: 20.4, speed: 1, volume: 1 },
    { id: 't2', assetId: macro.id, trackId: 'v1', start: 6.5, duration: 5.4, inPoint: 1.2, speed: 1, volume: 1 }
  ],
  captions: [caption('Made to be held.', 0.6, 3.4), caption('Built to disappear.', 7.2, 10.4)],
  scenes: [{ index: 0, assetId: broll.id, start: 20.4, end: 26.9 }],
  silences: [],
  settings: {
    captionsEnabled: true,
    captionPreset: 'bold',
    silencePadding: 0.1,
    autoRipple: true
  }
}

const projects = new Map<string, Project>([
  [studioProject.id, structuredClone(studioProject)],
  [teaserProject.id, structuredClone(teaserProject)]
])

const jobListeners = new Set<(job: Job) => void>()
const timers = new Map<string, number>()

function emit(job: Job): void {
  for (const listener of jobListeners) listener({ ...job })
}

function scriptFor(duration: number): Caption[] {
  const lines = [
    'The quiet parts are where the story lives.',
    'So clipfarm listens first, then cuts.',
    'Every pause becomes a decision you can undo.',
    'Word timings arrive with the audio.',
    'Nothing leaves this machine.'
  ]
  const captions: Caption[] = []
  const step = Math.max(4, Math.min(duration / lines.length, 7))
  lines.forEach((line, index) => {
    const start = 1.2 + index * step
    const end = start + Math.min(3.6, step - 0.6)
    if (end < duration) captions.push(caption(line, Number(start.toFixed(2)), Number(end.toFixed(2))))
  })
  return captions
}

function silencesFor(duration: number): Silence[] {
  const silences: Silence[] = []
  let cursor = 3.2
  while (cursor < duration - 1) {
    silences.push({ start: Number(cursor.toFixed(2)), end: Number((cursor + 1.8).toFixed(2)) })
    cursor += 11.5
  }
  return silences
}

function scenesFor(duration: number): Scene[] {
  const count = Math.max(2, Math.round(duration / 18))
  const step = duration / count
  return Array.from({ length: count }, (_, index) => ({
    index,
    start: Number((index * step).toFixed(2)),
    end: Number(((index + 1) * step).toFixed(2))
  }))
}

const engine: EngineStatus = {
  ready: false,
  python: 'python3 (preview)',
  pythonVersion: '3.12',
  ffmpeg: null,
  ffprobe: null,
  device: 'cpu',
  packages: { fasterWhisper: false, scenedetect: false, autoEditor: false, torch: false },
  message: 'Preview mode — media and analysis are simulated. Run the desktop app for local FFmpeg and Whisper.'
}

export function createDemoApi(): ClipfarmApi {
  const api: ClipfarmApi = {
    platform: 'browser',
    versions: { app: '0.1.0-preview', electron: '—', chrome: navigator.userAgent.split('Chrome/')[1]?.split(' ')[0] ?? '—' },
    listProjects: async () => [...projects.values()].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)),
    createProject: async (name) => {
      const project: Project = {
        schemaVersion: 1,
        id: `demo-${Date.now()}`,
        name: name || 'Untitled Project',
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        fps: 30,
        width: 1920,
        height: 1080,
        assets: [],
        tracks: [
          { id: 'v2', name: 'Overlay', kind: 'overlay', height: 58, muted: false, locked: false },
          { id: 'v1', name: 'Video', kind: 'video', height: 74, muted: false, locked: false },
          { id: 'cc', name: 'Captions', kind: 'captions', height: 46, muted: false, locked: false }
        ],
        clips: [],
        captions: [],
        scenes: [],
        silences: [],
        settings: { captionsEnabled: true, captionPreset: 'clean', silencePadding: 0.12, autoRipple: true }
      }
      projects.set(project.id, project)
      return structuredClone(project)
    },
    saveProject: async (project) => {
      const saved = { ...structuredClone(project), updatedAt: new Date().toISOString() }
      projects.set(saved.id, saved)
      return structuredClone(saved)
    },
    deleteProject: async (id) => {
      projects.delete(id)
    },
    importMedia: async () => [
      asset(`a-new-${Date.now()}`, 'City Night Drive.mp4', 92.6, { fps: 24 }),
      asset(`a-new-${Date.now() + 1}`, 'Ambient Bed.wav', 64, { kind: 'audio', width: 0, height: 0, fps: 0 })
    ],
    importPaths: async () => [],
    pathForFile: () => '',
    engineStatus: async () => engine,
    startJob: async (request: RunRequest) => {
      const job: Job = {
        id: `job-${Date.now()}`,
        kind: request.kind as RunKind,
        label:
          request.kind === 'silence'
            ? 'Removing silence'
            : request.kind === 'scenes'
              ? 'Detecting scenes'
              : request.kind === 'transcribe'
                ? 'Transcribing audio'
                : 'Exporting video',
        projectId: request.project.id,
        status: 'queued',
        progress: 0,
        stage: 'Queued',
        startedAt: Date.now()
      }
      emit(job)
      const assetId = 'assetId' in request ? request.assetId : request.project.assets[0]?.id
      const target = request.project.assets.find((item) => item.id === assetId)
      const duration = target?.duration || 60

      window.setTimeout(() => {
        job.status = 'running'
        job.stage = request.kind === 'transcribe' ? 'Loading small.en on CPU' : 'Working'
        emit(job)
        const tick = window.setInterval(() => {
          job.progress = Math.min(0.98, job.progress + 0.045 + Math.random() * 0.03)
          job.stage =
            request.kind === 'export'
              ? `Rendering · ${(job.progress * duration).toFixed(1)}s of ${duration.toFixed(0)}s`
              : request.kind === 'transcribe'
                ? 'Aligning words'
                : 'Analysing audio'
          emit(job)
        }, 220)

        timers.set(
          job.id,
          window.setTimeout(() => {
            window.clearInterval(tick)
            timers.delete(job.id)
            let result: JobResult
            switch (request.kind) {
              case 'silence':
                result = { kind: 'silence', assetId: assetId!, silences: silencesFor(duration) }
                break
              case 'scenes':
                result = { kind: 'scenes', assetId: assetId!, scenes: scenesFor(duration) }
                break
              case 'transcribe':
                result = {
                  kind: 'transcribe',
                  assetId: assetId!,
                  language: 'en',
                  captions: scriptFor(duration)
                }
                break
              default:
                result = {
                  kind: 'export',
                  outputPath: request.options.outputPath,
                  bytes: 148_000_000,
                  durationSeconds: 71.2
                }
            }
            job.result = result
            job.status = 'done'
            job.progress = 1
            job.stage = 'Done'
            job.finishedAt = Date.now()
            emit(job)
          }, 5200)
        )
      }, 350)
      return { ...job }
    },
    cancelJob: async (jobId) => {
      const timer = timers.get(jobId)
      if (timer) {
        window.clearTimeout(timer)
        timers.delete(jobId)
      }
    },
    onJobUpdate: (callback) => {
      jobListeners.add(callback)
      return () => jobListeners.delete(callback)
    },
    mediaUrl: () => '',
    revealFile: async () => undefined,
    openDataFolder: async () => undefined,
    mcpConfig: async (projectId): Promise<McpConfig> => {
      const projectPath = `~/Library/Application Support/clipfarm/projects/${projectId}.json`
      const args = ['-m', 'clipfarm_core', 'mcp', '--project', projectPath]
      return {
        command: 'python3',
        args,
        projectPath,
        config: JSON.stringify(
          { mcpServers: { clipfarm: { command: 'python3', args, env: { PYTHONPATH: './python' } } } },
          null,
          2
        )
      }
    },
    chooseExportPath: async () => '/Users/you/Movies/clipfarm-export.mp4',
    copyText: async (text) => {
      await navigator.clipboard?.writeText(text).catch(() => undefined)
    },
    onMenuCommand: () => () => undefined
  }
  return api
}

