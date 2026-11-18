import { app, Menu, shell, type MenuItemConstructorOptions } from 'electron'
import type { BrowserWindow } from 'electron'

function send(window: BrowserWindow | null, command: string): void {
  const target = window ?? null
  if (target && !target.isDestroyed()) target.webContents.send('menu:command', command)
}

/**
 * Every command in clipfarm is reachable from the menu bar, with the same
 * accelerator the UI shows in tooltips and the shortcuts sheet.
 */
export function buildApplicationMenu(getWindow: () => BrowserWindow | null): void {
  const isMac = process.platform === 'darwin'

  const template: MenuItemConstructorOptions[] = [
    ...(isMac
      ? [
          {
            label: app.name,
            submenu: [
              { role: 'about' as const },
              { type: 'separator' as const },
              { label: 'Settings…', accelerator: 'Cmd+,', click: () => send(getWindow(), 'settings') },
              { type: 'separator' as const },
              { role: 'services' as const },
              { type: 'separator' as const },
              { role: 'hide' as const },
              { role: 'hideOthers' as const },
              { role: 'unhide' as const },
              { type: 'separator' as const },
              { role: 'quit' as const }
            ]
          }
        ]
      : []),
    {
      label: 'File',
      submenu: [
        { label: 'New Project', accelerator: 'CmdOrCtrl+N', click: () => send(getWindow(), 'project.new') },
        {
          label: 'Import Media…',
          accelerator: 'CmdOrCtrl+I',
          click: () => send(getWindow(), 'media.import')
        },
        { type: 'separator' },
        { label: 'Save Project', accelerator: 'CmdOrCtrl+S', click: () => send(getWindow(), 'project.save') },
        { type: 'separator' },
        {
          label: 'Export Video…',
          accelerator: 'Shift+CmdOrCtrl+E',
          click: () => send(getWindow(), 'export.open')
        },
        ...(isMac
          ? []
          : [
              { type: 'separator' as const },
              { label: 'Settings', accelerator: 'Ctrl+,', click: () => send(getWindow(), 'settings') },
              { type: 'separator' as const },
              { role: 'quit' as const }
            ])
      ]
    },
    {
      label: 'Edit',
      submenu: [
        { label: 'Undo', accelerator: 'CmdOrCtrl+Z', click: () => send(getWindow(), 'edit.undo') },
        {
          label: 'Redo',
          accelerator: 'Shift+CmdOrCtrl+Z',
          click: () => send(getWindow(), 'edit.redo')
        },
        { type: 'separator' },
        { role: 'cut' },
        { role: 'copy' },
        { role: 'paste' },
        { role: 'selectAll' },
        { type: 'separator' },
        { label: 'Split Clip at Playhead', accelerator: 'CmdOrCtrl+B', click: () => send(getWindow(), 'timeline.split') },
        { label: 'Delete Selection', click: () => send(getWindow(), 'edit.delete') }
      ]
    },
    {
      label: 'View',
      submenu: [
        { label: 'Library', accelerator: 'CmdOrCtrl+1', click: () => send(getWindow(), 'view.projects') },
        { label: 'Media', accelerator: 'CmdOrCtrl+2', click: () => send(getWindow(), 'view.media') },
        { label: 'Transcript', accelerator: 'CmdOrCtrl+3', click: () => send(getWindow(), 'view.transcript') },
        { label: 'Agent Bridge', accelerator: 'CmdOrCtrl+4', click: () => send(getWindow(), 'view.agents') },
        { type: 'separator' },
        { label: 'Toggle Sidebar', accelerator: 'Alt+CmdOrCtrl+S', click: () => send(getWindow(), 'view.sidebar') },
        { label: 'Toggle Inspector', accelerator: 'Alt+CmdOrCtrl+I', click: () => send(getWindow(), 'view.inspector') },
        { type: 'separator' },
        { label: 'Actual Size', role: 'resetZoom' },
        { label: 'Zoom In', role: 'zoomIn' },
        { label: 'Zoom Out', role: 'zoomOut' },
        { type: 'separator' },
        { role: 'togglefullscreen' }
      ]
    },
    {
      label: 'AI',
      submenu: [
        {
          label: 'Remove Silence…',
          accelerator: 'Shift+CmdOrCtrl+D',
          click: () => send(getWindow(), 'ai.silence')
        },
        {
          label: 'Detect Scenes',
          accelerator: 'Shift+CmdOrCtrl+C',
          click: () => send(getWindow(), 'ai.scenes')
        },
        {
          label: 'Transcribe Audio',
          accelerator: 'CmdOrCtrl+T',
          click: () => send(getWindow(), 'ai.transcribe')
        },
        { type: 'separator' },
        { label: 'Command Palette…', accelerator: 'CmdOrCtrl+K', click: () => send(getWindow(), 'palette.open') }
      ]
    },
    {
      label: 'Window',
      submenu: [{ role: 'minimize' }, { role: 'zoom' }, ...(isMac ? [{ role: 'front' as const }] : [])]
    },
    {
      role: 'help',
      submenu: [
        { label: 'Keyboard Shortcuts', accelerator: 'CmdOrCtrl+/', click: () => send(getWindow(), 'help.shortcuts') },
        {
          label: 'MCP Agent Setup',
          click: () => send(getWindow(), 'help.mcp')
        },
        {
          label: 'Open Data Folder',
          click: () => {
            void shell.openPath(app.getPath('userData'))
          }
        }
      ]
    }
  ]

  Menu.setApplicationMenu(Menu.buildFromTemplate(template))
}
