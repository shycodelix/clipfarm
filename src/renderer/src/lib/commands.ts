export interface CommandDef {
  id: string
  title: string
  group: 'Project' | 'Edit' | 'AI' | 'View' | 'Help'
  shortcut?: string
  description?: string
}

/**
 * One registry powers the native menu, the command palette, the shortcuts sheet
 * and the keyboard handler, so an accelerator can never drift out of sync.
 */
export const COMMANDS: CommandDef[] = [
  { id: 'project.new', title: 'New Project', group: 'Project', shortcut: 'Mod+N' },
  { id: 'media.import', title: 'Import Media…', group: 'Project', shortcut: 'Mod+I' },
  { id: 'project.save', title: 'Save Project', group: 'Project', shortcut: 'Mod+S' },
  { id: 'project.close', title: 'Close Project', group: 'Project', shortcut: 'Shift+Mod+W' },
  { id: 'export.open', title: 'Export Video…', group: 'Project', shortcut: 'Shift+Mod+E' },
  { id: 'edit.undo', title: 'Undo', group: 'Edit', shortcut: 'Mod+Z' },
  { id: 'edit.redo', title: 'Redo', group: 'Edit', shortcut: 'Shift+Mod+Z' },
  { id: 'timeline.split', title: 'Split Clip at Playhead', group: 'Edit', shortcut: 'Mod+B' },
  { id: 'timeline.addCaption', title: 'Add Caption at Playhead', group: 'Edit', shortcut: 'Alt+Mod+C' },
  { id: 'edit.delete', title: 'Delete Selection', group: 'Edit', shortcut: 'Backspace' },
  { id: 'playback.play', title: 'Play / Pause', group: 'Edit', shortcut: 'Space' },
  { id: 'playback.back', title: 'Rewind', group: 'Edit', shortcut: 'J' },
  { id: 'playback.pause', title: 'Pause', group: 'Edit', shortcut: 'K' },
  { id: 'playback.forward', title: 'Play Forward', group: 'Edit', shortcut: 'L' },
  { id: 'timeline.prevScene', title: 'Previous Scene', group: 'Edit', shortcut: 'Alt+Left' },
  { id: 'timeline.nextScene', title: 'Next Scene', group: 'Edit', shortcut: 'Alt+Right' },
  {
    id: 'ai.silence',
    title: 'Remove Silence & Dead Air',
    group: 'AI',
    shortcut: 'Shift+Mod+D',
    description: 'Detect pauses with FFmpeg and ripple-cut them out'
  },
  {
    id: 'ai.scenes',
    title: 'Detect Scenes',
    group: 'AI',
    shortcut: 'Shift+Mod+C',
    description: 'Split the take with PySceneDetect'
  },
  {
    id: 'ai.transcribe',
    title: 'Transcribe with faster-whisper',
    group: 'AI',
    shortcut: 'Mod+T',
    description: 'Word-level captions on CPU or GPU'
  },
  { id: 'view.projects', title: 'Go to Library', group: 'View', shortcut: 'Mod+1' },
  { id: 'view.media', title: 'Show Media', group: 'View', shortcut: 'Mod+2' },
  { id: 'view.transcript', title: 'Show Transcript', group: 'View', shortcut: 'Mod+3' },
  { id: 'view.agents', title: 'Open Agent Bridge', group: 'View', shortcut: 'Mod+4' },
  { id: 'view.sidebar', title: 'Toggle Sidebar', group: 'View', shortcut: 'Alt+Mod+S' },
  { id: 'view.inspector', title: 'Toggle Inspector', group: 'View', shortcut: 'Alt+Mod+I' },
  { id: 'palette.open', title: 'Command Palette', group: 'View', shortcut: 'Mod+K' },
  { id: 'help.shortcuts', title: 'Keyboard Shortcuts', group: 'Help', shortcut: 'Mod+/' },
  { id: 'help.mcp', title: 'MCP Agent Setup', group: 'Help' },
  { id: 'settings', title: 'Settings', group: 'Help', shortcut: 'Mod+,' }
]

export function commandById(id: string): CommandDef | undefined {
  return COMMANDS.find((command) => command.id === id)
}

/** Canonical modifier order so "Shift+Mod+E" and "Mod+Shift+E" compare equal. */
export function normalizeShortcut(label: string): string {
  const parts = label.split('+')
  const key = parts.filter((part) => !['Mod', 'Ctrl', 'Alt', 'Shift'].includes(part)).join('+')
  const modifiers = ['Mod', 'Ctrl', 'Alt', 'Shift'].filter((modifier) => parts.includes(modifier))
  return [...modifiers, key].join('+')
}

/** Map a keyboard event onto a command shortcut string. */
export function shortcutFromEvent(event: KeyboardEvent | React.KeyboardEvent): string {
  const parts: string[] = []
  if (event.metaKey) parts.push('Mod')
  if (event.ctrlKey) parts.push(event.metaKey ? 'Ctrl' : 'Ctrl')
  if (event.altKey) parts.push('Alt')
  if (event.shiftKey) parts.push('Shift')
  const key = event.key
  if (['Shift', 'Control', 'Meta', 'Alt'].includes(key)) return parts.join('+')
  const normalized =
    key === ' '
      ? 'Space'
      : key === 'Backspace'
        ? 'Backspace'
        : key === 'ArrowLeft'
          ? 'Left'
          : key === 'ArrowRight'
            ? 'Right'
            : key === 'Escape'
              ? 'Esc'
              : key === '/'
                ? '/'
                : key.length === 1
                  ? key.toUpperCase()
                  : key
  parts.push(normalized)
  return parts.join('+')
}
