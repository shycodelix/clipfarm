import { app } from 'electron'
import { randomUUID } from 'node:crypto'
import { promises as fs } from 'node:fs'
import { join } from 'node:path'
import type { Project, Track } from '../shared/types'

const SCHEMA_VERSION = 1 as const

export function projectsDirectory(): string {
  return join(app.getPath('userData'), 'projects')
}

export function thumbnailsDirectory(): string {
  return join(app.getPath('userData'), 'thumbnails')
}

export async function ensureDirectories(): Promise<void> {
  await fs.mkdir(projectsDirectory(), { recursive: true })
  await fs.mkdir(thumbnailsDirectory(), { recursive: true })
}

export function defaultTracks(): Track[] {
  return [
    { id: 'v2', name: 'Overlay', kind: 'overlay', height: 58, muted: false, locked: false },
    { id: 'v1', name: 'Video', kind: 'video', height: 74, muted: false, locked: false },
    { id: 'cc', name: 'Captions', kind: 'captions', height: 46, muted: false, locked: false }
  ]
}

export function createProject(name: string): Project {
  const now = new Date().toISOString()
  return {
    schemaVersion: SCHEMA_VERSION,
    id: randomUUID(),
    name: name.trim() || 'Untitled Project',
    createdAt: now,
    updatedAt: now,
    fps: 30,
    width: 1920,
    height: 1080,
    assets: [],
    tracks: defaultTracks(),
    clips: [],
    captions: [],
    scenes: [],
    silences: [],
    settings: {
      captionsEnabled: true,
      captionPreset: 'clean',
      silencePadding: 0.12,
      autoRipple: true
    }
  }
}

export function projectPath(id: string): string {
  return join(projectsDirectory(), `${id}.json`)
}

function isProject(value: unknown): value is Project {
  if (!value || typeof value !== 'object') return false
  const candidate = value as Partial<Project>
  return (
    typeof candidate.id === 'string' &&
    typeof candidate.name === 'string' &&
    Array.isArray(candidate.assets) &&
    Array.isArray(candidate.clips) &&
    Array.isArray(candidate.tracks)
  )
}

function normalize(project: Project): Project {
  const settings: Project['settings'] = {
    captionsEnabled: true,
    captionPreset: 'clean',
    silencePadding: 0.12,
    autoRipple: true
  }
  if (project.settings) Object.assign(settings, project.settings)
  return {
    ...project,
    schemaVersion: SCHEMA_VERSION,
    captions: project.captions ?? [],
    scenes: project.scenes ?? [],
    silences: project.silences ?? [],
    settings
  }
}

export async function listProjects(): Promise<Project[]> {
  await ensureDirectories()
  const entries = await fs.readdir(projectsDirectory()).catch(() => [] as string[])
  const projects: Project[] = []
  for (const entry of entries) {
    if (!entry.endsWith('.json')) continue
    try {
      const raw = await fs.readFile(join(projectsDirectory(), entry), 'utf8')
      const parsed: unknown = JSON.parse(raw)
      if (isProject(parsed)) projects.push(normalize(parsed))
    } catch {
      // A corrupt project should never take the library down with it.
    }
  }
  return projects.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
}

export async function readProject(id: string): Promise<Project | null> {
  try {
    const raw = await fs.readFile(projectPath(id), 'utf8')
    const parsed: unknown = JSON.parse(raw)
    return isProject(parsed) ? normalize(parsed) : null
  } catch {
    return null
  }
}

export async function saveProject(project: Project): Promise<Project> {
  await ensureDirectories()
  const next: Project = {
    ...normalize(project),
    updatedAt: new Date().toISOString()
  }
  const target = projectPath(next.id)
  const temporary = `${target}.${process.pid}.tmp`
  await fs.writeFile(temporary, JSON.stringify(next, null, 2), 'utf8')
  await fs.rename(temporary, target)
  return next
}

export async function deleteProject(id: string): Promise<void> {
  await fs.rm(projectPath(id), { force: true })
}
