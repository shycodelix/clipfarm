import { randomUUID } from 'node:crypto'
import { BrowserWindow } from 'electron'
import type {
  ExportOptions,
  Job,
  JobKind,
  JobResult,
  RunRequest,
  SilenceOptions,
  TranscribeOptions
} from '../shared/types'
import { engine, offlineStatus } from './engine'

const jobs = new Map<string, Job>()
const activeChains = new Map<string, Promise<void>>()

function broadcast(job: Job): void {
  for (const window of BrowserWindow.getAllWindows()) {
    if (!window.isDestroyed()) window.webContents.send('job:update', job)
  }
}

function labelFor(request: RunRequest): string {
  switch (request.kind) {
    case 'silence':
      return 'Removing silence'
    case 'scenes':
      return 'Detecting scenes'
    case 'transcribe':
      return 'Transcribing audio'
    case 'export':
      return 'Exporting video'
  }
}

export function getJob(id: string): Job | undefined {
  return jobs.get(id)
}

export async function startJob(request: RunRequest): Promise<Job> {
  const job: Job = {
    id: randomUUID(),
    kind: request.kind as JobKind,
    label: labelFor(request),
    projectId: request.project.id,
    status: 'queued',
    progress: 0,
    stage: 'Queued',
    startedAt: Date.now()
  }
  jobs.set(job.id, job)
  broadcast(job)
  const chain = run(job, request).finally(() => activeChains.delete(job.id))
  activeChains.set(job.id, chain)
  return job
}

async function run(job: Job, request: RunRequest): Promise<void> {
  job.status = 'running'
  job.stage = 'Preparing engine'
  broadcast(job)

  const onProgress = (progress: number, stage?: string): void => {
    job.progress = Math.min(1, Math.max(0, progress))
    if (stage) job.stage = stage
    broadcast(job)
  }

  try {
    let result: JobResult
    switch (request.kind) {
      case 'silence': {
        const response = await engine.request<{ silences: NonNullable<JobResult & { kind: 'silence' }>['silences'] }>(
          'analyze.silence',
          {
            jobId: job.id,
            path: assetPathOf(request),
            options: {
              thresholdDb: -34,
              minSilence: 0.35,
              padding: request.project.settings.silencePadding ?? 0.12,
              ...(request.options ?? {})
            } satisfies SilenceOptions
          },
          { timeoutMs: 30 * 60_000, onProgress }
        )
        result = { kind: 'silence', assetId: request.assetId, silences: response.silences }
        break
      }
      case 'scenes': {
        const response = await engine.request<{ scenes: NonNullable<JobResult & { kind: 'scenes' }>['scenes'] }>(
          'analyze.scenes',
          {
            jobId: job.id,
            path: assetPathOf(request),
            options: { threshold: 27, minSceneLength: 0.6, ...(request.options ?? {}) }
          },
          { timeoutMs: 30 * 60_000, onProgress }
        )
        result = { kind: 'scenes', assetId: request.assetId, scenes: response.scenes }
        break
      }
      case 'transcribe': {
        const response = await engine.request<{
          language: string
          captions: NonNullable<JobResult & { kind: 'transcribe' }>['captions']
        }>(
          'transcribe',
          {
            jobId: job.id,
            path: assetPathOf(request),
            options: {
              model: 'small.en',
              language: 'auto',
              computeType: 'int8',
              vad: true,
              ...(request.options ?? {})
            } satisfies TranscribeOptions
          },
          { timeoutMs: 6 * 60 * 60_000, onProgress }
        )
        result = {
          kind: 'transcribe',
          assetId: request.assetId,
          language: response.language,
          captions: response.captions
        }
        break
      }
      case 'export': {
        const response = await engine.request<{
          outputPath: string
          bytes: number
          durationSeconds: number
        }>(
          'export',
          { jobId: job.id, project: request.project, options: request.options satisfies ExportOptions },
          { timeoutMs: 6 * 60 * 60_000, onProgress }
        )
        result = { kind: 'export', ...response }
        break
      }
    }

    job.result = result
    job.status = 'done'
    job.progress = 1
    job.stage = 'Done'
    job.finishedAt = Date.now()
    broadcast(job)
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    // cancelJob() mutates the job from outside this async run, so widen the check.
    if ((job.status as string) === 'cancelled') {
      broadcast(job)
      return
    }
    job.status = 'error'
    job.error = message
    job.stage = 'Failed'
    job.finishedAt = Date.now()
    broadcast(job)
  }
}

function assetPathOf(request: Extract<RunRequest, { assetId: string }>): string {
  const asset = request.project.assets.find((candidate) => candidate.id === request.assetId)
  if (!asset?.path) {
    throw new Error('This asset has no file on disk. Import the source media to run analysis.')
  }
  return asset.path
}

export async function cancelJob(id: string): Promise<void> {
  const job = jobs.get(id)
  if (!job) return
  if (job.status === 'done' || job.status === 'error') return
  job.status = 'cancelled'
  job.stage = 'Cancelled'
  job.finishedAt = Date.now()
  broadcast(job)
  try {
    await engine.request('cancel', { jobId: id }, { timeoutMs: 10_000 })
  } catch {
    // Cancellation is best effort; the UI already reflects the stopped job.
  }
}

export function engineStatusOrOffline(message: string) {
  return offlineStatus(message)
}
