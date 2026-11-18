import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process'
import { EventEmitter } from 'node:events'
import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { app } from 'electron'
import type { EngineStatus } from '../shared/types'

export function pythonDirectory(): string {
  return app.isPackaged
    ? join(process.resourcesPath, 'python')
    : join(app.getAppPath(), 'python')
}

function venvPython(root: string): string {
  return process.platform === 'win32'
    ? join(root, '.venv', 'Scripts', 'python.exe')
    : join(root, '.venv', 'bin', 'python')
}

/**
 * Prefer an explicit interpreter, then the project virtualenv, then the system
 * Python. The engine itself reports which optional packages are importable, so a
 * bare interpreter still yields a useful status instead of a hard failure.
 */
export function resolvePython(): string {
  const explicit = process.env.CLIPFARM_PYTHON
  if (explicit) return explicit
  const candidates = [venvPython(app.getAppPath()), venvPython(process.cwd())]
  for (const candidate of candidates) {
    if (existsSync(candidate)) return candidate
  }
  return process.platform === 'win32' ? 'python' : 'python3'
}

export function offlineStatus(message: string): EngineStatus {
  return {
    ready: false,
    python: null,
    pythonVersion: null,
    ffmpeg: null,
    ffprobe: null,
    device: 'unknown',
    packages: { fasterWhisper: false, scenedetect: false, autoEditor: false, torch: false },
    message
  }
}

interface PendingRequest {
  resolve: (value: unknown) => void
  reject: (error: Error) => void
  timer: NodeJS.Timeout
  onProgress?: (progress: number, stage?: string) => void
}

type EngineEvents = {
  progress: [id: number, progress: number, stage: string | undefined]
  log: [level: string, message: string]
  exit: []
}

/**
 * Long-lived Python sidecar speaking newline-delimited JSON-RPC over stdio.
 * Starting it once and reusing it keeps model loading (Whisper) warm across jobs.
 */
class PythonEngine extends EventEmitter<EngineEvents> {
  private child: ChildProcessWithoutNullStreams | null = null
  private buffer = ''
  private nextId = 1
  private pending = new Map<number, PendingRequest>()
  private starting: Promise<void> | null = null

  async start(): Promise<void> {
    if (this.child) return
    if (this.starting) return this.starting

    this.starting = new Promise<void>((resolve, reject) => {
      const python = resolvePython()
      const cwd = pythonDirectory()
      let child: ChildProcessWithoutNullStreams
      try {
        child = spawn(python, ['-u', '-m', 'clipfarm_core', 'serve'], {
          cwd,
          env: {
            ...process.env,
            PYTHONPATH: cwd,
            PYTHONUNBUFFERED: '1',
            PYTHONIOENCODING: 'utf-8'
          }
        })
      } catch (error) {
        this.starting = null
        reject(error instanceof Error ? error : new Error(String(error)))
        return
      }

      this.child = child
      let settled = false

      child.stdout.setEncoding('utf8')
      child.stderr.setEncoding('utf8')
      child.stderr.on('data', (chunk: string) => {
        const text = chunk.trim()
        if (text) console.warn('[engine]', text.slice(0, 2000))
      })
      child.stdout.on('data', (chunk: string) => {
        this.buffer += chunk
        let newline = this.buffer.indexOf('\n')
        while (newline >= 0) {
          const line = this.buffer.slice(0, newline).trim()
          this.buffer = this.buffer.slice(newline + 1)
          if (line) this.handleLine(line)
          newline = this.buffer.indexOf('\n')
        }
      })
      child.once('error', (error) => {
        this.failAll(new Error(`Python engine could not start: ${error.message}`))
        this.child = null
        this.starting = null
        if (!settled) {
          settled = true
          reject(error)
        }
      })
      child.once('exit', (code) => {
        this.child = null
        this.starting = null
        this.failAll(new Error(`Python engine exited (code ${code ?? 'null'})`))
        this.emit('exit')
      })

      // The engine prints a ready line once its stdio loop is running.
      const onReady = (level: string, message: string): void => {
        if (level !== 'ready') return
        this.off('log', onReady)
        if (!settled) {
          settled = true
          resolve()
        }
        void message
      }
      this.on('log', onReady)
      setTimeout(() => {
        this.off('log', onReady)
        if (!settled) {
          settled = true
          resolve()
        }
      }, 2500)
    }).finally(() => {
      this.starting = null
    })

    return this.starting
  }

  private handleLine(line: string): void {
    let message: Record<string, unknown>
    try {
      message = JSON.parse(line) as Record<string, unknown>
    } catch {
      return
    }

    if (message.event === 'log') {
      this.emit('log', String(message.level ?? 'info'), String(message.message ?? ''))
      return
    }
    if (message.event === 'progress') {
      const id = Number(message.id)
      const progress = Number(message.progress ?? 0)
      const stage = message.stage ? String(message.stage) : undefined
      this.pending.get(id)?.onProgress?.(Number.isFinite(progress) ? progress : 0, stage)
      this.emit('progress', id, progress, stage)
      return
    }

    const id = Number(message.id)
    const request = this.pending.get(id)
    if (!request) return
    this.pending.delete(id)
    clearTimeout(request.timer)
    if (message.ok === true) {
      request.resolve(message.result)
    } else {
      const error = message.error as { message?: string; code?: string } | undefined
      request.reject(new Error(error?.message ?? 'The engine reported an unknown error'))
    }
  }

  private failAll(error: Error): void {
    for (const [id, request] of this.pending) {
      clearTimeout(request.timer)
      request.reject(error)
      this.pending.delete(id)
    }
  }

  async request<T>(
    method: string,
    params: unknown,
    options: { timeoutMs?: number; onProgress?: (progress: number, stage?: string) => void } = {}
  ): Promise<T> {
    await this.start()
    const child = this.child
    if (!child) throw new Error('The Python engine is not running')

    const id = this.nextId++
    const timeoutMs = options.timeoutMs ?? 120_000
    return new Promise<T>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id)
        reject(new Error(`Engine request "${method}" timed out`))
      }, timeoutMs)
      this.pending.set(id, {
        resolve: (value) => resolve(value as T),
        reject,
        timer,
        onProgress: options.onProgress
      })
      child.stdin.write(`${JSON.stringify({ id, method, params })}\n`)
    })
  }

  async status(): Promise<EngineStatus> {
    return this.request<EngineStatus>('status', {}, { timeoutMs: 20_000 })
  }

  stop(): void {
    this.child?.kill()
    this.child = null
  }
}

export const engine = new PythonEngine()
