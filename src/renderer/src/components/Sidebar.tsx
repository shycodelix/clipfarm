import {
  Bot,
  Captions,
  FileText,
  FolderOpen,
  Keyboard,
  LayoutGrid,
  Plus,
  Settings,
  Sparkles
} from 'lucide-react'
import type { ReactNode } from 'react'
import { clock, isMac } from '../lib/format'
import { useStudio } from '../store'
import { cn, IconButton } from './primitives'

function SectionLabel({ children }: { children: ReactNode }): ReactNode {
  return (
    <div className="px-3 pb-1 pt-4 text-[10.5px] font-semibold uppercase tracking-[0.06em] text-tertiary">
      {children}
    </div>
  )
}

interface RowProps {
  icon: ReactNode
  label: string
  meta?: string
  selected?: boolean
  onClick: () => void
}

function Row({ icon, label, meta, selected, onClick }: RowProps): ReactNode {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-current={selected ? 'page' : undefined}
      className={cn(
        'flex h-[30px] w-full items-center gap-2 rounded-[6px] px-2 text-left text-[12px] transition-colors',
        selected
          ? 'bg-accent font-medium text-white shadow-[0_1px_2px_rgba(0,0,0,0.12)]'
          : 'text-secondary hover:bg-elevated hover:text-label'
      )}
    >
      <span className={cn('flex w-4 shrink-0 justify-center', selected ? 'text-white' : 'text-tertiary')}>
        {icon}
      </span>
      <span className="truncate">{label}</span>
      {meta ? (
        <span className={cn('ml-auto shrink-0 text-[10.5px]', selected ? 'text-white/75' : 'text-tertiary')}>
          {meta}
        </span>
      ) : null}
    </button>
  )
}

export function Sidebar(): ReactNode {
  const { state, actions, isDemo } = useStudio()
  const project = state.project
  const recent = state.projects.slice(0, 4)
  const engineReady = state.engine?.ready && !isDemo

  return (
    <aside className="material-chrome flex w-[228px] shrink-0 flex-col border-r border-separator">
      <div className="flex-1 overflow-y-auto px-2 pb-3">
        <div className="px-1 pb-1 pt-3">
          <button
            type="button"
            onClick={() => actions.setModal({ kind: 'new-project' })}
            className="flex h-8 w-full items-center gap-2 rounded-[7px] bg-accent px-2.5 text-[12.5px] font-medium text-white shadow-[0_1px_2px_rgba(0,0,0,0.15)] transition hover:brightness-110"
          >
            <Plus size={14} aria-hidden />
            New Project
          </button>
        </div>

        <SectionLabel>Library</SectionLabel>
        <nav className="flex flex-col gap-px" aria-label="Library">
          <Row
            icon={<LayoutGrid size={13} aria-hidden />}
            label="All Projects"
            meta={String(state.projects.length)}
            selected={state.view === 'library'}
            onClick={() => actions.setView('library')}
          />
        </nav>

        <SectionLabel>Recent</SectionLabel>
        <nav className="flex flex-col gap-px" aria-label="Recent projects">
          {recent.length === 0 ? (
            <p className="px-3 py-1 text-[11.5px] text-tertiary">No projects yet</p>
          ) : (
            recent.map((item) => (
              <Row
                key={item.id}
                icon={<FolderOpen size={13} aria-hidden />}
                label={item.name}
                meta={clock(Math.max(item.clips.reduce((max, clip) => Math.max(max, clip.start + clip.duration), 0), 0))}
                selected={state.view === 'editor' && project?.id === item.id}
                onClick={() => void actions.openProject(item.id)}
              />
            ))
          )}
        </nav>

        {project ? (
          <>
            <SectionLabel>Workspace</SectionLabel>
            <nav className="flex flex-col gap-px" aria-label="Workspace">
              <Row
                icon={<FolderOpen size={13} aria-hidden />}
                label="Media"
                selected={state.view === 'editor' && state.leftPanel === 'media'}
                onClick={() => {
                  actions.setView('editor')
                  actions.setLeftPanel('media')
                }}
              />
              <Row
                icon={<FileText size={13} aria-hidden />}
                label="Transcript"
                meta={project.captions.length ? String(project.captions.length) : undefined}
                selected={state.view === 'editor' && state.leftPanel === 'transcript'}
                onClick={() => {
                  actions.setView('editor')
                  actions.setLeftPanel('transcript')
                }}
              />
              <Row
                icon={<Captions size={13} aria-hidden />}
                label="Captions"
                selected={state.inspectorVisible && state.inspectorTab === 'captions'}
                onClick={() => {
                  actions.setView('editor')
                  actions.setInspectorTab('captions')
                }}
              />
              <Row
                icon={<Sparkles size={13} aria-hidden />}
                label="AI Tools"
                selected={state.inspectorVisible && state.inspectorTab === 'ai'}
                onClick={() => {
                  actions.setView('editor')
                  actions.setInspectorTab('ai')
                }}
              />
            </nav>
          </>
        ) : null}

        <SectionLabel>Automation</SectionLabel>
        <nav className="flex flex-col gap-px" aria-label="Automation">
          <Row
            icon={<Bot size={13} aria-hidden />}
            label="Agent Bridge"
            selected={state.view === 'agents'}
            onClick={() => actions.setView('agents')}
          />
        </nav>
      </div>

      <div className="border-t border-separator px-2 py-2">
        <button
          type="button"
          onClick={() => actions.setModal({ kind: 'settings' })}
          className="flex w-full items-center gap-2 rounded-[6px] px-2 py-1.5 text-left hover:bg-elevated"
          title="Engine settings"
        >
          <span
            className={cn(
              'h-1.5 w-1.5 shrink-0 rounded-full',
              engineReady ? 'bg-success' : isDemo ? 'bg-warning' : 'bg-danger'
            )}
          />
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[11px] font-medium text-label">
              {isDemo ? 'Preview engine' : engineReady ? 'Local engine ready' : 'Engine needs setup'}
            </span>
            <span className="block truncate text-[10.5px] text-tertiary">
              {isDemo
                ? 'Simulated analysis'
                : state.engine?.device
                  ? `${state.engine.device.toUpperCase()} · whisper ${state.engine.packages.fasterWhisper ? 'ready' : 'off'}`
                  : 'Checking…'}
            </span>
          </span>
        </button>
        <div className="mt-1 flex items-center gap-0.5">
          <IconButton label="Settings" shortcut="Mod+," size="sm" onClick={() => actions.setModal({ kind: 'settings' })}>
            <Settings size={13} aria-hidden />
          </IconButton>
          <IconButton
            label="Keyboard shortcuts"
            shortcut="Mod+/"
            size="sm"
            onClick={() => actions.setModal({ kind: 'shortcuts' })}
          >
            <Keyboard size={13} aria-hidden />
          </IconButton>
          <span className="ml-auto pr-1 text-[10px] text-tertiary">{isMac() ? 'macOS' : 'Desktop'} · 0.1</span>
        </div>
      </div>
    </aside>
  )
}
