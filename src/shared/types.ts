/**
 * Shared contract between the Electron main process, the preload bridge and the
 * React renderer. The Python engine mirrors this schema (see docs/PROJECT_FORMAT.md),
 * so keep both sides in sync when a field changes.
 */

export type TrackKind = 'video' | 'overlay' | 'captions'
export type MediaKind = 'video' | 'audio' | 'image'

export interface MediaAsset {
  id: string
  name: string
  /** Absolute path on disk. Empty for demo assets that have no file behind them. */
  path: string
  kind: MediaKind
  /** Seconds. 0 when the duration is not known yet. */
  duration: number
  width: number
  height: number
  fps: number
  hasAudio: boolean
  thumbnailPath?: string
}

export interface Clip {
  id: string
  assetId: string
  trackId: string
  /** Seconds on the timeline. */
  start: number
  /** Seconds of timeline occupied by the clip. */
  duration: number
  /** Seconds into the source asset where playback begins. */
  inPoint: number
  speed: number
  volume: number
  label?: string
}

export interface Word {
  text: string
  start: number
  end: number
  confidence?: number
}

export interface Caption {
  id: string
  start: number
  end: number
  text: string
  words: Word[]
}

export interface Scene {
  index: number
  /** Source asset these boundaries belong to. */
  assetId?: string
  start: number
  end: number
  score?: number
}

export interface Silence {
  start: number
  end: number
}

export interface Track {
  id: string
  name: string
  kind: TrackKind
  height: number
  muted: boolean
  locked: boolean
}

export interface ProjectSettings {
  captionsEnabled: boolean
  captionPreset: 'clean' | 'bold' | 'minimal'
  silencePadding: number
  autoRipple: boolean
}

export interface Project {
  schemaVersion: 1
  id: string
  name: string
  createdAt: string
  updatedAt: string
  fps: number
  width: number
  height: number
  assets: MediaAsset[]
  tracks: Track[]
  clips: Clip[]
  captions: Caption[]
  scenes: Scene[]
  silences: Silence[]
  settings: ProjectSettings
}

export type JobKind = 'silence' | 'scenes' | 'transcribe' | 'export'
export type JobStatus = 'queued' | 'running' | 'done' | 'error' | 'cancelled'

export interface SilenceOptions {
  thresholdDb: number
  minSilence: number
  padding: number
}

export interface SceneOptions {
  threshold: number
  minSceneLength: number
}

export interface TranscribeOptions {
  model: string
  language: string | 'auto'
  computeType: 'int8' | 'float16' | 'float32'
  vad: boolean
}

export interface ExportOptions {
  outputPath: string
  container: 'mp4' | 'mov'
  resolution: 'source' | '1080p' | '4k' | 'vertical'
  quality: 'high' | 'balanced' | 'small'
  burnCaptions: boolean
  fps: number
}

export type JobResult =
  | { kind: 'silence'; assetId: string; silences: Silence[] }
  | { kind: 'scenes'; assetId: string; scenes: Scene[] }
  | { kind: 'transcribe'; assetId: string; language: string; captions: Caption[] }
  | { kind: 'export'; outputPath: string; bytes: number; durationSeconds: number }

export interface Job {
  id: string
  kind: JobKind
  label: string
  projectId: string
  status: JobStatus
  /** 0..1 */
  progress: number
  stage?: string
  error?: string
  result?: JobResult
  startedAt: number
  finishedAt?: number
}

export type RunRequest =
  | { kind: 'silence'; project: Project; assetId: string; options?: Partial<SilenceOptions> }
  | { kind: 'scenes'; project: Project; assetId: string; options?: Partial<SceneOptions> }
  | { kind: 'transcribe'; project: Project; assetId: string; options?: Partial<TranscribeOptions> }
  | { kind: 'export'; project: Project; options: ExportOptions }

export interface EngineStatus {
  ready: boolean
  python: string | null
  pythonVersion: string | null
  ffmpeg: string | null
  ffprobe: string | null
  device: 'cpu' | 'cuda' | 'mps' | 'unknown'
  packages: {
    fasterWhisper: boolean
    scenedetect: boolean
    autoEditor: boolean
    torch: boolean
  }
  message: string
}

export interface McpConfig {
  command: string
  args: string[]
  projectPath: string
  /** Ready to paste into an MCP client config file. */
  config: string
}

export interface ClipfarmApi {
  /** Node platform id in the desktop app, "browser" for the preview backend. */
  platform: string
  versions: { app: string; electron: string; chrome: string }
  listProjects(): Promise<Project[]>
  createProject(name: string): Promise<Project>
  saveProject(project: Project): Promise<Project>
  deleteProject(id: string): Promise<void>
  importMedia(): Promise<MediaAsset[]>
  importPaths(paths: string[]): Promise<MediaAsset[]>
  pathForFile(file: File): string
  engineStatus(): Promise<EngineStatus>
  startJob(request: RunRequest): Promise<Job>
  cancelJob(jobId: string): Promise<void>
  onJobUpdate(callback: (job: Job) => void): () => void
  mediaUrl(path: string): string
  revealFile(path: string): Promise<void>
  openDataFolder(): Promise<void>
  mcpConfig(projectId: string): Promise<McpConfig>
  chooseExportPath(suggestedName: string): Promise<string | null>
  copyText(text: string): Promise<void>
  onMenuCommand(callback: (id: string) => void): () => void
}
