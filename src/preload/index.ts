import { contextBridge, ipcRenderer, webUtils } from 'electron'
import type {
  ClipfarmApi,
  EngineStatus,
  Job,
  McpConfig,
  MediaAsset,
  Project,
  RunRequest
} from '../shared/types'

const api: ClipfarmApi = {
  platform: process.platform,
  versions: {
    app: process.env.npm_package_version ?? '0.1.0',
    electron: process.versions.electron,
    chrome: process.versions.chrome
  },
  listProjects: () => ipcRenderer.invoke('projects:list') as Promise<Project[]>,
  createProject: (name: string) => ipcRenderer.invoke('projects:create', name) as Promise<Project>,
  saveProject: (project: Project) => ipcRenderer.invoke('projects:save', project) as Promise<Project>,
  deleteProject: (id: string) => ipcRenderer.invoke('projects:delete', id) as Promise<void>,
  importMedia: () => ipcRenderer.invoke('media:import') as Promise<MediaAsset[]>,
  importPaths: (paths: string[]) => ipcRenderer.invoke('media:import-paths', paths) as Promise<MediaAsset[]>,
  pathForFile: (file: File) => {
    try {
      return webUtils.getPathForFile(file)
    } catch {
      return ''
    }
  },
  engineStatus: () => ipcRenderer.invoke('engine:status') as Promise<EngineStatus>,
  startJob: (request: RunRequest) => ipcRenderer.invoke('jobs:start', request) as Promise<Job>,
  cancelJob: (jobId: string) => ipcRenderer.invoke('jobs:cancel', jobId) as Promise<void>,
  onJobUpdate: (callback: (job: Job) => void) => {
    const listener = (_event: Electron.IpcRendererEvent, job: Job): void => callback(job)
    ipcRenderer.on('job:update', listener)
    return () => {
      ipcRenderer.removeListener('job:update', listener)
    }
  },
  mediaUrl: (path: string) => `clipfarm-media://local/?path=${encodeURIComponent(path)}`,
  revealFile: (path: string) => ipcRenderer.invoke('shell:reveal', path) as Promise<void>,
  openDataFolder: () => ipcRenderer.invoke('shell:open-data-folder') as Promise<void>,
  mcpConfig: (projectId: string) => ipcRenderer.invoke('mcp:config', projectId) as Promise<McpConfig>,
  chooseExportPath: (suggestedName: string) =>
    ipcRenderer.invoke('export:choose-path', suggestedName) as Promise<string | null>,
  copyText: (text: string) => ipcRenderer.invoke('clipboard:write', text) as Promise<void>,
  onMenuCommand: (callback: (id: string) => void) => {
    const listener = (_event: Electron.IpcRendererEvent, id: string): void => callback(id)
    ipcRenderer.on('menu:command', listener)
    return () => {
      ipcRenderer.removeListener('menu:command', listener)
    }
  }
}

contextBridge.exposeInMainWorld('clipfarm', api)
