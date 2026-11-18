import { useCallback, useEffect, useRef, useState, type DragEvent, type ReactNode } from 'react'
import { Upload } from 'lucide-react'
import { AgentBridge } from './components/AgentBridge'
import { Editor } from './components/Editor'
import { Library } from './components/Library'
import { StatusBar, Toasts, renderModal } from './components/Overlays'
import { JobBar } from './components/Overlays'
import { Sidebar } from './components/Sidebar'
import { TitleBar } from './components/TitleBar'
import { api } from './lib/bridge'
import { COMMANDS, normalizeShortcut, shortcutFromEvent } from './lib/commands'
import { sceneMarkers, totalDuration } from './lib/timeline'
import { useStudio } from './store'

export default function App(): ReactNode {
  const { state, actions } = useStudio()
  const [fileDrag, setFileDrag] = useState(false)
  const dragDepth = useRef(0)
  const playheadRef = useRef(state.playhead)
  playheadRef.current = state.playhead

  const runCommand = useCallback(
    (id: string): void => {
      const project = state.project
      switch (id) {
        case 'project.new':
          actions.setModal({ kind: 'new-project' })
          break
        case 'media.import':
          void actions.importMedia()
          break
        case 'project.save':
          void actions.saveNow()
          break
        case 'project.close':
          actions.closeProject()
          break
        case 'export.open':
          if (project) actions.openExport()
          break
        case 'edit.undo':
          actions.undo()
          break
        case 'edit.redo':
          actions.redo()
          break
        case 'timeline.split':
          actions.splitAtPlayhead()
          break
        case 'timeline.addCaption':
          actions.addCaptionAtPlayhead()
          break
        case 'edit.delete':
          actions.deleteSelection()
          break
        case 'playback.play':
          actions.togglePlay()
          break
        case 'playback.back':
          actions.setPlaying(true)
          actions.step(-1)
          break
        case 'playback.pause':
          actions.setPlaying(false)
          break
        case 'playback.forward':
          actions.setPlaying(true)
          break
        case 'timeline.prevScene':
        case 'timeline.nextScene': {
          if (!project) break
          const markers = sceneMarkers(project)
          const direction = id === 'timeline.nextScene' ? 1 : -1
          const ordered = direction === 1 ? markers : [...markers].reverse()
          const next =
            ordered.find((marker) =>
              direction === 1 ? marker > state.playhead + 0.05 : marker < state.playhead - 0.05
            ) ?? (direction === 1 ? totalDuration(project) : 0)
          actions.seek(next)
          break
        }
        case 'ai.silence':
          if (project) void actions.runJob('silence')
          break
        case 'ai.scenes':
          if (project) void actions.runJob('scenes')
          break
        case 'ai.transcribe':
          if (project) void actions.runJob('transcribe')
          break
        case 'view.projects':
          actions.setView('library')
          break
        case 'view.media':
          actions.setView('editor')
          actions.setLeftPanel('media')
          break
        case 'view.transcript':
          actions.setView('editor')
          actions.setLeftPanel('transcript')
          break
        case 'view.agents':
          actions.setView('agents')
          break
        case 'view.sidebar':
          actions.toggleSidebar()
          break
        case 'view.inspector':
          actions.toggleInspector()
          break
        case 'palette.open':
          actions.setModal({ kind: 'palette' })
          break
        case 'help.shortcuts':
          actions.setModal({ kind: 'shortcuts' })
          break
        case 'help.mcp':
          actions.setView('agents')
          break
        case 'settings':
          actions.setModal({ kind: 'settings' })
          break
        default:
          break
      }
    },
    [actions, state.playhead, state.project]
  )

  // Playback clock. Runs while playing and stops at the end of the timeline.
  useEffect(() => {
    if (!state.playing || !state.project) return
    const duration = totalDuration(state.project)
    const startedAt = performance.now()
    const from = playheadRef.current
    let frame = 0
    const tick = (now: number): void => {
      const next = from + (now - startedAt) / 1000
      if (next >= duration) {
        actions.seek(duration)
        actions.setPlaying(false)
        return
      }
      actions.seek(next)
      frame = requestAnimationFrame(tick)
    }
    frame = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(frame)
  }, [state.playing, state.project?.id, actions])

  // Native menu commands arrive with the same ids the palette and shortcuts use.
  useEffect(() => api.onMenuCommand((id) => runCommand(id)), [runCommand])

  // Global shortcuts. Focus in a text field always wins, except for Escape.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        if (state.modal) {
          actions.setModal(null)
          return
        }
        actions.selectClip(null)
        actions.selectCaption(null)
        return
      }

      const target = event.target as HTMLElement | null
      const typing =
        target !== null &&
        (target.tagName === 'INPUT' ||
          target.tagName === 'TEXTAREA' ||
          target.tagName === 'SELECT' ||
          target.isContentEditable)
      if (typing) return

      const pressed = normalizeShortcut(shortcutFromEvent(event))
      const command = COMMANDS.find(
        (candidate) => candidate.shortcut && normalizeShortcut(candidate.shortcut) === pressed
      )
      if (!command) return
      event.preventDefault()
      runCommand(command.id)
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [actions, runCommand, state.modal])

  const hasFiles = (event: DragEvent): boolean =>
    Array.from(event.dataTransfer.types ?? []).includes('Files')

  return (
    <div
      className="relative flex h-full flex-col overflow-hidden bg-content text-label"
      onDragEnter={(event) => {
        if (!hasFiles(event)) return
        dragDepth.current += 1
        setFileDrag(true)
      }}
      onDragOver={(event) => {
        if (hasFiles(event)) event.preventDefault()
      }}
      onDragLeave={() => {
        dragDepth.current = Math.max(0, dragDepth.current - 1)
        if (dragDepth.current === 0) setFileDrag(false)
      }}
      onDrop={(event) => {
        dragDepth.current = 0
        setFileDrag(false)
        if (!event.dataTransfer.files?.length) return
        event.preventDefault()
        void actions.dropFiles(Array.from(event.dataTransfer.files))
      }}
    >
      <TitleBar />
      <div className="flex min-h-0 flex-1">
        {state.sidebarVisible ? <Sidebar /> : null}
        <main className="flex min-w-0 flex-1 flex-col">
          {state.view === 'library' ? (
            <Library />
          ) : state.view === 'agents' ? (
            <AgentBridge />
          ) : (
            <Editor />
          )}
          <JobBar />
          <StatusBar />
        </main>
      </div>

      {fileDrag ? (
        <div className="pointer-events-none fixed inset-3 z-[60] flex items-center justify-center rounded-[14px] border-2 border-dashed border-accent bg-accent-soft/60 backdrop-blur-sm">
          <div className="flex flex-col items-center gap-2 rounded-[12px] bg-surface/90 px-6 py-5 shadow-[var(--shadow-float)]">
            <Upload size={22} aria-hidden className="text-accent" />
            <div className="text-[13px] font-semibold text-label">Drop to import</div>
            <p className="text-[11px] text-secondary">Video, audio and images stay on this machine.</p>
          </div>
        </div>
      ) : null}

      <Toasts />
      {renderModal(state.modal, runCommand)}
    </div>
  )
}
