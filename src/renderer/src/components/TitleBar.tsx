import { Download, PanelLeft, PanelRight, Search } from 'lucide-react'
import type { ReactNode } from 'react'
import { isMac, shortcut as shortcutLabel } from '../lib/format'
import { useStudio } from '../store'
import { Badge, Button, IconButton } from './primitives'

function SavedState(): ReactNode {
  const { state } = useStudio()
  if (state.saving) return <span className="text-[11px] text-tertiary">Saving…</span>
  if (state.dirty) return <span className="text-[11px] text-tertiary">Edited</span>
  if (state.lastSavedAt) return <span className="text-[11px] text-tertiary">Saved</span>
  return null
}

export function TitleBar(): ReactNode {
  const { state, actions, isDemo } = useStudio()
  const mac = isMac()

  return (
    <header className="material-chrome drag-region relative z-30 flex h-12 shrink-0 items-center gap-3 border-b border-separator px-3">
      <div className={mac ? 'w-[68px]' : 'w-0'} aria-hidden />
      <div className="flex min-w-0 items-center gap-2.5">
        <div className="flex h-6 w-6 items-center justify-center rounded-[6px] bg-accent text-white shadow-[0_1px_2px_rgba(0,0,0,0.2)]">
          <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden>
            <rect x="2" y="3" width="12" height="10" rx="2.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
            <path d="M6.6 6.2 10 8l-3.4 1.8z" fill="currentColor" />
          </svg>
        </div>
        <div className="flex min-w-0 items-baseline gap-2">
          <span className="text-[12.5px] font-semibold tracking-[-0.01em] text-label">clipfarm</span>
          {state.project ? (
            <>
              <span className="text-tertiary" aria-hidden>
                /
              </span>
              <span className="max-w-[280px] truncate text-[12.5px] text-secondary">
                {state.project.name}
              </span>
              <SavedState />
            </>
          ) : null}
        </div>
        {isDemo ? <Badge tone="accent">Preview</Badge> : null}
      </div>

      <div className="no-drag ml-auto flex items-center gap-1">
        <button
          type="button"
          onClick={() => actions.setModal({ kind: 'palette' })}
          className="mr-1 hidden h-7 items-center gap-2 rounded-[7px] border border-separator bg-surface/80 px-2 text-[11.5px] text-secondary hover:border-line hover:text-label sm:flex"
          title={`Search commands (${shortcutLabel('Mod+K')})`}
        >
          <Search size={12} aria-hidden />
          <span>Commands</span>
          <kbd className="rounded-[4px] bg-elevated px-1 text-[10px] text-tertiary">
            {shortcutLabel('Mod+K')}
          </kbd>
        </button>
        <IconButton
          label={state.sidebarVisible ? 'Hide sidebar' : 'Show sidebar'}
          shortcut="Ctrl+Mod+S"
          active={state.sidebarVisible}
          onClick={actions.toggleSidebar}
        >
          <PanelLeft size={15} aria-hidden />
        </IconButton>
        <IconButton
          label={state.inspectorVisible ? 'Hide inspector' : 'Show inspector'}
          shortcut="Alt+Mod+I"
          active={state.inspectorVisible}
          onClick={actions.toggleInspector}
        >
          <PanelRight size={15} aria-hidden />
        </IconButton>
        <Button
          variant="primary"
          size="sm"
          icon={<Download size={13} aria-hidden />}
          onClick={actions.openExport}
          disabled={!state.project}
          className="ml-1"
        >
          Export
        </Button>
      </div>
    </header>
  )
}
