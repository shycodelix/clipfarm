import { app, BrowserWindow, clipboard, dialog, ipcMain, nativeTheme, protocol, shell } from 'electron'
import { existsSync, createReadStream } from 'node:fs'
import { stat } from 'node:fs/promises'
import { join, resolve, extname } from 'node:path'
import { Readable } from 'node:stream'
import { pathToFileURL } from 'node:url'
import type { Job, McpConfig, Project, RunRequest } from '../shared/types'
import { buildApplicationMenu } from './menu'
import { engine, offlineStatus, pythonDirectory, resolvePython } from './engine'
import { chooseMedia, ensureThumbnailDirectory, importPaths } from './media'
import { cancelJob, startJob } from './jobs'
import {
  createProject,
  deleteProject,
  ensureDirectories,
  listProjects,
  readProject,
  saveProject
} from './projectStore'

const MEDIA_SCHEME = 'clipfarm-media'
const PLAYABLE_EXTENSIONS = new Set([
  '.mp4', '.mov', '.mkv', '.webm', '.m4v', '.avi',
  '.wav', '.mp3', '.m4a', '.aac', '.flac', '.aiff', '.ogg',
  '.png', '.jpg', '.jpeg', '.webp', '.gif', '.heic'
])

let mainWindow: BrowserWindow | null = null

protocol.registerSchemesAsPrivileged([
  {
    scheme: MEDIA_SCHEME,
    privileges: { standard: true, secure: true, supportFetchAPI: true, stream: true }
  }
])

function createWindow(): void {
  const isMac = process.platform === 'darwin'
  mainWindow = new BrowserWindow({
    width: 1480,
    height: 940,
    minWidth: 1200,
    minHeight: 760,
    show: false,
    title: 'clipfarm',
    backgroundColor: nativeTheme.shouldUseDarkColors ? '#141416' : '#f1f1f3',
    titleBarStyle: isMac ? 'hiddenInset' : 'default',
    trafficLightPosition: isMac ? { x: 16, y: 17 } : undefined,
    vibrancy: isMac ? 'under-window' : undefined,
    visualEffectState: isMac ? 'followWindow' : undefined,
    webPreferences: {
      preload: join(__dirname, '../preload/index.mjs'),
      sandbox: false,
      contextIsolation: true,
      nodeIntegration: false,
      spellcheck: true
    }
  })

  mainWindow.once('ready-to-show', () => mainWindow?.show())

  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    if (url.startsWith('https://')) void shell.openExternal(url)
    return { action: 'deny' }
  })

  const devUrl = process.env['ELECTRON_RENDERER_URL']
  if (devUrl) {
    void mainWindow.loadURL(devUrl)
  } else {
    void mainWindow.loadFile(join(__dirname, '../renderer/index.html'))
  }

  mainWindow.on('closed', () => {
    mainWindow = null
  })
}

function registerMediaProtocol(): void {
  protocol.handle(MEDIA_SCHEME, async (request) => {
    try {
      const url = new URL(request.url)
      const raw = url.searchParams.get('path')
      if (!raw) return new Response('Missing path', { status: 400 })
      const filePath = resolve(decodeURIComponent(raw))
      const allowed =
        PLAYABLE_EXTENSIONS.has(extname(filePath).toLowerCase()) &&
        (existsSync(filePath) || filePath.includes('thumbnails'))
      if (!allowed) return new Response('Not found', { status: 404 })

      const info = await stat(filePath)
      if (!info.isFile()) return new Response('Not found', { status: 404 })

      const contentType = contentTypeFor(filePath)
      const range = request.headers.get('Range')
      if (range) {
        const match = /bytes=(\d*)-(\d*)/.exec(range)
        if (match) {
          const start = match[1] ? Number(match[1]) : 0
          const end = match[2] ? Math.min(Number(match[2]), info.size - 1) : info.size - 1
          if (Number.isFinite(start) && start <= end) {
            const stream = createReadStream(filePath, { start, end })
            return new Response(Readable.toWeb(stream) as ReadableStream, {
              status: 206,
              headers: {
                'Content-Type': contentType,
                'Content-Length': String(end - start + 1),
                'Content-Range': `bytes ${start}-${end}/${info.size}`,
                'Accept-Ranges': 'bytes',
                'Cache-Control': 'no-store'
              }
            })
          }
        }
      }

      const stream = createReadStream(filePath)
      return new Response(Readable.toWeb(stream) as ReadableStream, {
        status: 200,
        headers: {
          'Content-Type': contentType,
          'Content-Length': String(info.size),
          'Accept-Ranges': 'bytes',
          'Cache-Control': 'no-store'
        }
      })
    } catch {
      return new Response('Unavailable', { status: 500 })
    }
  })
}

function contentTypeFor(path: string): string {
  switch (extname(path).toLowerCase()) {
    case '.mp4':
    case '.m4v':
      return 'video/mp4'
    case '.mov':
      return 'video/quicktime'
    case '.webm':
      return 'video/webm'
    case '.mkv':
      return 'video/x-matroska'
    case '.mp3':
      return 'audio/mpeg'
    case '.wav':
      return 'audio/wav'
    case '.m4a':
      return 'audio/mp4'
    case '.flac':
      return 'audio/flac'
    case '.ogg':
      return 'audio/ogg'
    case '.png':
      return 'image/png'
    case '.webp':
      return 'image/webp'
    case '.gif':
      return 'image/gif'
    case '.jpg':
    case '.jpeg':
      return 'image/jpeg'
    default:
      return 'application/octet-stream'
  }
}

function mcpConfigFor(project: Project): McpConfig {
  const python = resolvePython()
  const projectPath = join(app.getPath('userData'), 'projects', `${project.id}.json`)
  const args = ['-m', 'clipfarm_core', 'mcp', '--project', projectPath]
  const config = JSON.stringify(
    {
      mcpServers: {
        clipfarm: {
          command: python,
          args,
          env: { PYTHONPATH: pythonDirectory() }
        }
      }
    },
    null,
    2
  )
  return { command: python, args, projectPath, config }
}

function registerIpc(): void {
  ipcMain.handle('projects:list', () => listProjects())
  ipcMain.handle('projects:create', (_event, name: string) => saveProject(createProject(name)))
  ipcMain.handle('projects:read', (_event, id: string) => readProject(id))
  ipcMain.handle('projects:save', (_event, project: Project) => saveProject(project))
  ipcMain.handle('projects:delete', (_event, id: string) => deleteProject(id))

  ipcMain.handle('media:import', async () => {
    const paths = await chooseMedia(mainWindow)
    if (!paths.length) return []
    const status = await engine.status().catch(() => offlineStatus('Engine unavailable'))
    const assets = await importPaths(paths, status.ffmpeg)
    return assets
  })

  ipcMain.handle('media:import-paths', async (_event, paths: string[]) => {
    const status = await engine.status().catch(() => offlineStatus('Engine unavailable'))
    return importPaths(paths, status.ffmpeg)
  })

  ipcMain.handle('engine:status', async () => {
    try {
      return await engine.status()
    } catch (error) {
      return offlineStatus(
        error instanceof Error
          ? `${error.message}. Install the media engine with: npm run python:setup`
          : 'The local media engine is unavailable.'
      )
    }
  })

  ipcMain.handle('jobs:start', (_event, request: RunRequest) => startJob(request))
  ipcMain.handle('jobs:cancel', (_event, id: string) => cancelJob(id))

  ipcMain.handle('export:choose-path', async (_event, suggestedName: string) => {
    const options: Electron.SaveDialogOptions = {
      title: 'Export video',
      buttonLabel: 'Export',
      defaultPath: join(app.getPath('videos'), suggestedName),
      filters: [{ name: 'MP4 video', extensions: ['mp4'] }]
    }
    const result = mainWindow
      ? await dialog.showSaveDialog(mainWindow, options)
      : await dialog.showSaveDialog(options)
    return result.canceled ? null : result.filePath
  })

  ipcMain.handle('mcp:config', async (_event, projectId: string) => {
    const project = await readProject(projectId)
    if (!project) throw new Error('Save the project before connecting an agent')
    return mcpConfigFor(project)
  })

  ipcMain.handle('shell:reveal', (_event, path: string) => {
    if (path && existsSync(path)) shell.showItemInFolder(path)
  })

  ipcMain.handle('shell:open-data-folder', () => shell.openPath(app.getPath('userData')))

  ipcMain.handle('clipboard:write', (_event, text: string) => clipboard.writeText(text))
}

app.whenReady().then(async () => {
  await ensureDirectories()
  await ensureThumbnailDirectory()
  registerMediaProtocol()
  registerIpc()
  buildApplicationMenu(() => mainWindow)
  createWindow()

  // Warm the sidecar so the first analysis does not pay interpreter startup.
  void engine.status().catch(() => undefined)

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit()
})

app.on('before-quit', () => {
  engine.stop()
})
