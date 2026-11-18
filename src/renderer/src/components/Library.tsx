import { Clock, Film, Plus, Trash2, Upload } from 'lucide-react'
import { useMemo, type ReactNode } from 'react'
import type { Project } from '@shared/types'
import { clock, gradientFor, initials } from '../lib/format'
import { useStudio } from '../store'
import { Button, EmptyState, IconButton } from './primitives'

function relative(iso: string): string {
  const then = new Date(iso).getTime()
  if (Number.isNaN(then)) return 'Just now'
  const minutes = Math.max(0, Math.round((Date.now() - then) / 60_000))
  if (minutes < 1) return 'Just now'
  if (minutes < 60) return `${minutes} min ago`
  const hours = Math.round(minutes / 60)
  if (hours < 24) return `${hours} hr ago`
  const days = Math.round(hours / 24)
  if (days === 1) return 'Yesterday'
  if (days < 30) return `${days} days ago`
  return new Date(iso).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
}

function durationOf(project: Project): number {
  return project.clips.reduce((max, clip) => Math.max(max, clip.start + clip.duration), 0)
}

function ProjectCard({ project }: { project: Project }): ReactNode {
  const { actions, state } = useStudio()
  const [from, to] = gradientFor(project.name)
  const duration = durationOf(project)
  const active = state.project?.id === project.id

  return (
    <article
      className={`group relative overflow-hidden rounded-[11px] border bg-surface text-left transition-shadow ${
        active ? 'border-accent/50 shadow-[0_0_0_1px_var(--accent-soft)]' : 'border-separator hover:shadow-lg'
      }`}
    >
      <button
        type="button"
        onClick={() => void actions.openProject(project.id)}
        className="block w-full text-left"
        aria-label={`Open ${project.name}`}
      >
        <div
          className="relative flex aspect-video items-center justify-center"
          style={{ background: `linear-gradient(145deg, ${from}, ${to})` }}
        >
          <span className="text-[22px] font-semibold tracking-tight text-white/90">
            {initials(project.name)}
          </span>
          <span className="absolute bottom-2 right-2 rounded-[4px] bg-black/55 px-1.5 py-0.5 text-[10.5px] font-medium tabular-nums text-white/90">
            {duration ? clock(duration) : 'Empty'}
          </span>
          {project.captions.length ? (
            <span className="absolute left-2 top-2 rounded-[4px] bg-white/15 px-1.5 py-0.5 text-[10px] font-medium text-white backdrop-blur">
              {project.captions.length} captions
            </span>
          ) : null}
        </div>
        <div className="flex flex-col gap-0.5 px-3 py-2.5">
          <span className="truncate text-[12.5px] font-semibold text-label">{project.name}</span>
          <span className="flex items-center gap-2 text-[11px] text-secondary">
            <span className="inline-flex items-center gap-1">
              <Film size={11} aria-hidden />
              {project.clips.length} clips
            </span>
            <span className="text-tertiary" aria-hidden>
              ·
            </span>
            <span className="inline-flex items-center gap-1">
              <Clock size={11} aria-hidden />
              {relative(project.updatedAt)}
            </span>
          </span>
        </div>
      </button>
      <IconButton
        label={`Delete ${project.name}`}
        size="sm"
        className="absolute right-2 top-2 bg-black/35 text-white opacity-0 backdrop-blur hover:bg-black/55 hover:text-white group-hover:opacity-100 focus-visible:opacity-100"
        onClick={() => actions.setModal({ kind: 'delete-project', projectId: project.id })}
      >
        <Trash2 size={13} aria-hidden />
      </IconButton>
    </article>
  )
}

export function Library(): ReactNode {
  const { state, actions } = useStudio()
  const projects = useMemo(
    () => [...state.projects].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)),
    [state.projects]
  )

  return (
    <div className="flex-1 overflow-y-auto">
      <div className="mx-auto w-full max-w-[1040px] px-8 pb-16 pt-8">
        <div className="flex items-end justify-between gap-6">
          <div>
            <h1 className="text-[24px] font-semibold tracking-[-0.02em] text-label">Projects</h1>
            <p className="mt-1 text-[12.5px] text-secondary">
              Everything stays on this Mac. Import a take and clipfarm finds the pauses.
            </p>
          </div>
          <div className="flex items-center gap-2">
            <Button icon={<Upload size={13} aria-hidden />} onClick={() => void actions.importMedia()}>
              Import media
            </Button>
            <Button
              variant="primary"
              icon={<Plus size={13} aria-hidden />}
              onClick={() => actions.setModal({ kind: 'new-project' })}
            >
              New Project
            </Button>
          </div>
        </div>

        <div className="mt-7">
          {projects.length === 0 ? (
            <div className="rounded-[13px] border border-dashed border-line bg-surface/50">
              <EmptyState
                icon={<Film size={20} aria-hidden />}
                title="No projects yet"
                message="Create a project, drop in an interview, and let the engine remove the dead air for you."
                action={
                  <Button variant="primary" onClick={() => actions.setModal({ kind: 'new-project' })}>
                    Create your first project
                  </Button>
                }
              />
            </div>
          ) : (
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
              {projects.map((project) => (
                <ProjectCard key={project.id} project={project} />
              ))}
            </div>
          )}
        </div>

        <div className="mt-8 flex items-center gap-2 rounded-[10px] border border-separator bg-surface/60 px-3.5 py-3">
          <span className="flex h-7 w-7 items-center justify-center rounded-[6px] bg-accent-soft text-accent">
            <Film size={14} aria-hidden />
          </span>
          <p className="text-[11.5px] leading-relaxed text-secondary">
            <strong className="font-semibold text-label">Local by default.</strong> Media, transcripts and
            renders never leave your machine. The AI engine runs through FFmpeg, faster-whisper and
            PySceneDetect on your own CPU or GPU.
          </p>
        </div>
      </div>
    </div>
  )
}
