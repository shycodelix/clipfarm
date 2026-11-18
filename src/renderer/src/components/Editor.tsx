import { Film, Plus } from 'lucide-react'
import type { ReactNode } from 'react'
import { useStudio } from '../store'
import { Inspector } from './Inspector'
import { LeftPanel } from './LeftPanel'
import { Preview } from './Preview'
import { Timeline } from './Timeline'
import { Button, EmptyState } from './primitives'

function EmptyEditor(): ReactNode {
  const { actions } = useStudio()
  return (
    <div className="flex flex-1 items-center justify-center">
      <div className="rounded-[13px] border border-dashed border-line bg-surface/50 px-10">
        <EmptyState
          icon={<Film size={20} aria-hidden />}
          title="Nothing open"
          message="Choose a project from the library, or start a new one and drop in a take."
          action={
            <div className="flex items-center gap-2">
              <Button onClick={() => actions.setView('library')}>Open Library</Button>
              <Button
                variant="primary"
                icon={<Plus size={13} aria-hidden />}
                onClick={() => actions.setModal({ kind: 'new-project' })}
              >
                New Project
              </Button>
            </div>
          }
        />
      </div>
    </div>
  )
}

export function Editor(): ReactNode {
  const { state } = useStudio()
  if (!state.project) return <EmptyEditor />

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex min-h-0 flex-1">
        <LeftPanel />
        <Preview />
        {state.inspectorVisible ? <Inspector /> : null}
      </div>
      <Timeline />
    </div>
  )
}
