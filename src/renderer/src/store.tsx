import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  type ReactElement,
  type ReactNode
} from 'react'
import type {
  EngineStatus,
  ExportOptions,
  Job,
  JobKind,
  MediaAsset,
  Project,
  RunRequest,
  SceneOptions,
  SilenceOptions,
  TranscribeOptions
} from '@shared/types'
import { api, isDemo } from './lib/bridge'
import {
  addCaption as addCaptionEdit,
  appendAssetToTrack,
  applyScenes,
  deleteCaption,
  deleteClip,
  findClip,
  removeSilences,
  replaceCaptions,
  splitClipAt
} from './lib/edits'
import { clamp, shortcut } from './lib/format'
import { totalDuration, videoTrack } from './lib/timeline'

export type View = 'library' | 'editor' | 'agents'
export type LeftPanel = 'media' | 'transcript'
export type InspectorTab = 'clip' | 'captions' | 'ai'
export type Modal =
  | null
  | { kind: 'export' }
  | { kind: 'palette' }
  | { kind: 'shortcuts' }
  | { kind: 'settings' }
  | { kind: 'new-project' }
  | { kind: 'delete-project'; projectId: string }

export interface Toast {
  id: string
  kind: 'info' | 'success' | 'error'
  message: string
  detail?: string
}

interface State {
  booted: boolean
  projects: Project[]
  project: Project | null
  view: View
  leftPanel: LeftPanel
  inspectorTab: InspectorTab
  sidebarVisible: boolean
  inspectorVisible: boolean
  selectedClipId: string | null
  selectedCaptionId: string | null
  playhead: number
  playing: boolean
  zoom: number
  dirty: boolean
  saving: boolean
  lastSavedAt: string | null
  history: { past: Project[]; future: Project[] }
  job: Job | null
  engine: EngineStatus | null
  toasts: Toast[]
  modal: Modal
}

const initialState: State = {
  booted: false,
  projects: [],
  project: null,
  view: 'library',
  leftPanel: 'media',
  inspectorTab: 'clip',
  sidebarVisible: true,
  inspectorVisible: true,
  selectedClipId: null,
  selectedCaptionId: null,
  playhead: 0,
  playing: false,
  zoom: 34,
  dirty: false,
  saving: false,
  lastSavedAt: null,
  history: { past: [], future: [] },
  job: null,
  engine: null,
  toasts: [],
  modal: null
}

type Action =
  | { type: 'hydrate'; projects: Project[]; engine: EngineStatus }
  | { type: 'projects'; projects: Project[] }
  | { type: 'open'; project: Project }
  | { type: 'view'; view: View }
  | { type: 'leftPanel'; panel: LeftPanel }
  | { type: 'inspectorTab'; tab: InspectorTab }
  | { type: 'sidebar'; visible?: boolean }
  | { type: 'inspector'; visible?: boolean }
  | { type: 'selectClip'; id: string | null }
  | { type: 'selectCaption'; id: string | null }
  | { type: 'playhead'; time: number }
  | { type: 'playing'; playing: boolean }
  | { type: 'zoom'; zoom: number }
  | { type: 'mutate'; mutate: (project: Project) => void; transient?: boolean }
  | { type: 'commit'; previous: Project }
  | { type: 'undo' }
  | { type: 'redo' }
  | { type: 'saving' }
  | { type: 'saved'; project: Project }
  | { type: 'job'; job: Job }
  | { type: 'clearJob' }
  | { type: 'toast'; toast: Toast }
  | { type: 'dismissToast'; id: string }
  | { type: 'modal'; modal: Modal }
  | { type: 'engine'; engine: EngineStatus }

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case 'hydrate':
      return { ...state, booted: true, projects: action.projects, engine: action.engine }
    case 'projects':
      return { ...state, projects: action.projects }
    case 'open':
      return {
        ...state,
        project: action.project,
        view: 'editor',
        playhead: 0,
        playing: false,
        selectedClipId: action.project.clips[0]?.id ?? null,
        dirty: false,
        history: { past: [], future: [] },
        modal: null
      }
    case 'view':
      return { ...state, view: action.view, playing: action.view === 'editor' ? state.playing : false }
    case 'leftPanel':
      return { ...state, leftPanel: action.panel, sidebarVisible: true }
    case 'inspectorTab':
      return { ...state, inspectorTab: action.tab, inspectorVisible: true }
    case 'sidebar':
      return { ...state, sidebarVisible: action.visible ?? !state.sidebarVisible }
    case 'inspector':
      return { ...state, inspectorVisible: action.visible ?? !state.inspectorVisible }
    case 'selectClip':
      return { ...state, selectedClipId: action.id, selectedCaptionId: null }
    case 'selectCaption':
      return { ...state, selectedCaptionId: action.id, selectedClipId: null, inspectorTab: 'captions', inspectorVisible: true }
    case 'playhead':
      return { ...state, playhead: Math.max(0, action.time) }
    case 'playing':
      return { ...state, playing: action.playing }
    case 'zoom':
      return { ...state, zoom: clamp(action.zoom, 8, 240) }
    case 'mutate': {
      if (!state.project) return state
      const previous = state.project
      const next = structuredClone(previous)
      action.mutate(next)
      next.updatedAt = new Date().toISOString()
      return {
        ...state,
        project: next,
        dirty: true,
        history: action.transient
          ? state.history
          : {
              past: [...state.history.past.slice(-49), structuredClone(previous)],
              future: []
            }
      }
    }
    case 'commit':
      return {
        ...state,
        history: {
          past: [...state.history.past.slice(-49), action.previous],
          future: []
        }
      }
    case 'undo': {
      const previous = state.history.past[state.history.past.length - 1]
      if (!previous || !state.project) return state
      return {
        ...state,
        project: previous,
        dirty: true,
        history: {
          past: state.history.past.slice(0, -1),
          future: [structuredClone(state.project), ...state.history.future]
        }
      }
    }
    case 'redo': {
      const next = state.history.future[0]
      if (!next || !state.project) return state
      return {
        ...state,
        project: next,
        dirty: true,
        history: {
          past: [...state.history.past, structuredClone(state.project)],
          future: state.history.future.slice(1)
        }
      }
    }
    case 'saving':
      return { ...state, saving: true }
    case 'saved':
      return {
        ...state,
        project: state.project?.id === action.project.id ? action.project : state.project,
        projects: state.projects.map((project) =>
          project.id === action.project.id ? action.project : project
        ),
        dirty: false,
        saving: false,
        lastSavedAt: new Date().toISOString()
      }
    case 'job':
      return { ...state, job: action.job }
    case 'clearJob':
      return { ...state, job: null }
    case 'toast':
      return { ...state, toasts: [...state.toasts.slice(-3), action.toast] }
    case 'dismissToast':
      return { ...state, toasts: state.toasts.filter((toast) => toast.id !== action.id) }
    case 'modal':
      return { ...state, modal: action.modal, playing: action.modal ? false : state.playing }
    case 'engine':
      return { ...state, engine: action.engine }
    default:
      return state
  }
}

interface Studio {
  state: State
  actions: {
    openProject: (id: string) => Promise<void>
    createProject: (name: string) => Promise<void>
    deleteProject: (id: string) => Promise<void>
    closeProject: () => void
    saveNow: () => Promise<void>
    importMedia: () => Promise<void>
    dropFiles: (files: File[]) => Promise<void>
    addAssetToTimeline: (asset: MediaAsset) => void
    setView: (view: View) => void
    setLeftPanel: (panel: LeftPanel) => void
    setInspectorTab: (tab: InspectorTab) => void
    toggleSidebar: () => void
    toggleInspector: () => void
    selectClip: (id: string | null) => void
    selectCaption: (id: string | null) => void
    seek: (time: number) => void
    step: (seconds: number) => void
    togglePlay: () => void
    setPlaying: (playing: boolean) => void
    setZoom: (zoom: number) => void
    mutate: (mutate: (project: Project) => void, transient?: boolean) => void
    commitHistory: (previous: Project) => void
    undo: () => void
    redo: () => void
    splitAtPlayhead: () => void
    deleteSelection: () => void
    addCaptionAtPlayhead: () => void
    runJob: (
      kind: JobKind,
      targetAssetId?: string,
      options?: {
        silence?: Partial<SilenceOptions>
        scenes?: Partial<SceneOptions>
        transcribe?: Partial<TranscribeOptions>
      }
    ) => Promise<void>
    cancelJob: () => Promise<void>
    dismissJob: () => void
    openExport: () => void
    startExport: (options: Omit<ExportOptions, 'outputPath'>) => Promise<void>
    copy: (text: string, label: string) => Promise<void>
    toast: (kind: Toast['kind'], message: string, detail?: string) => void
    dismissToast: (id: string) => void
    setModal: (modal: Modal) => void
    refreshEngine: () => Promise<void>
  }
  targetAssetId: string | null
  isDemo: boolean
}

const StudioContext = createContext<Studio | null>(null)

export function StudioProvider({ children }: { children: ReactNode }): ReactElement {
  const [state, dispatch] = useReducer(reducer, initialState)
  const stateRef = useRef(state)
  stateRef.current = state

  const toast = useCallback(
    (kind: Toast['kind'], message: string, detail?: string) => {
      dispatch({
        type: 'toast',
        toast: { id: `toast-${Date.now()}-${Math.random().toString(16).slice(2)}`, kind, message, detail }
      })
    },
    []
  )

  const refreshProjects = useCallback(async () => {
    const projects = await api.listProjects()
    dispatch({ type: 'projects', projects })
  }, [])

  // Bootstrap: library, engine diagnostics and the most recently touched project.
  useEffect(() => {
    let cancelled = false
    void (async () => {
      const [projects, engine] = await Promise.all([
        api.listProjects().catch(() => [] as Project[]),
        api.engineStatus().catch(() => null)
      ])
      if (cancelled) return
      dispatch({
        type: 'hydrate',
        projects,
        engine:
          engine ??
          ({
            ready: false,
            python: null,
            pythonVersion: null,
            ffmpeg: null,
            ffprobe: null,
            device: 'unknown',
            packages: { fasterWhisper: false, scenedetect: false, autoEditor: false, torch: false },
            message: 'The local engine did not respond.'
          } satisfies EngineStatus)
      })
      if (projects[0]) {
        dispatch({ type: 'open', project: projects[0] })
      }
    })()
    return () => {
      cancelled = true
    }
  }, [])

  // Debounced autosave. HIG: people should never have to think about saving.
  useEffect(() => {
    if (!state.project || !state.dirty) return
    const project = state.project
    const timer = window.setTimeout(() => {
      dispatch({ type: 'saving' })
      void api
        .saveProject(project)
        .then((saved) => {
          dispatch({ type: 'saved', project: saved })
          void refreshProjects()
        })
        .catch((error: unknown) => {
          dispatch({ type: 'saving' })
          toast('error', 'Could not save the project', error instanceof Error ? error.message : undefined)
        })
    }, 650)
    return () => window.clearTimeout(timer)
  }, [state.project, state.dirty, refreshProjects, toast])

  // Job results fold back into the timeline, always through an undoable mutation.
  useEffect(() => {
    return api.onJobUpdate((job) => {
      dispatch({ type: 'job', job })
      const current = stateRef.current
      if (job.status === 'done' && job.result && current.project?.id === job.projectId) {
        const result = job.result
        if (result.kind === 'silence') {
          dispatch({
            type: 'mutate',
            mutate: (project) => {
              project.silences = result.silences
              const { removed, cuts } = removeSilences(project, result.silences, result.assetId)
              if (cuts === 0) project.silences = result.silences
              window.setTimeout(
                () =>
                  toast(
                    cuts ? 'success' : 'info',
                    cuts ? `Removed ${removed.toFixed(1)}s of dead air` : 'No dead air found',
                    cuts
                      ? `Press ${shortcut('Mod+Z')} to restore the original timing.`
                      : 'Try a softer noise floor.'
                  ),
                10
              )
            }
          })
        } else if (result.kind === 'scenes') {
          dispatch({
            type: 'mutate',
            mutate: (project) => applyScenes(project, result.scenes, result.assetId)
          })
          toast('success', `${result.scenes.length} scenes detected`, 'Scene boundaries are marked on the timeline.')
        } else if (result.kind === 'transcribe') {
          dispatch({
            type: 'mutate',
            mutate: (project) => replaceCaptions(project, result.captions)
          })
          dispatch({ type: 'leftPanel', panel: 'transcript' })
          toast(
            'success',
            `${result.captions.length} captions added`,
            `Language ${result.language.toUpperCase()} · word-level timing`
          )
        } else if (result.kind === 'export') {
          toast('success', 'Export finished', result.outputPath)
        }
      }
      if (job.status === 'error' && job.error) {
        toast('error', `${job.label} failed`, job.error)
      }
      if (job.status === 'cancelled') {
        toast('info', `${job.label} cancelled`)
      }
    })
  }, [toast])

  const targetAssetId = useMemo(() => {
    if (!state.project) return null
    const selected = findClip(state.project, state.selectedClipId)
    if (selected) return selected.assetId
    const track = videoTrack(state.project)
    const clip = state.project.clips.find((item) => item.trackId === track.id)
    if (clip) return clip.assetId
    return state.project.assets.find((asset) => asset.path || asset.duration > 0)?.id ?? null
  }, [state.project, state.selectedClipId])

  const actions = useMemo<Studio['actions']>(() => {
    const mutate = (mutator: (project: Project) => void, transient = false): void => {
      dispatch({ type: 'mutate', mutate: mutator, transient })
    }

    const seek = (time: number): void => {
      const duration = totalDuration(stateRef.current.project)
      dispatch({ type: 'playhead', time: clamp(time, 0, duration || 0) })
    }

    return {
      openProject: async (id) => {
        const existing = stateRef.current.projects.find((project) => project.id === id)
        const project = existing ?? (await api.listProjects()).find((item) => item.id === id)
        if (!project) {
          toast('error', 'That project could not be opened')
          return
        }
        dispatch({ type: 'open', project })
      },
      createProject: async (name) => {
        try {
          const project = await api.createProject(name)
          dispatch({ type: 'projects', projects: [project, ...stateRef.current.projects] })
          dispatch({ type: 'open', project })
          toast('success', 'Project created', 'Import media to start the timeline.')
        } catch (error) {
          toast('error', 'Could not create the project', error instanceof Error ? error.message : undefined)
        }
      },
      deleteProject: async (id) => {
        await api.deleteProject(id)
        const remaining = stateRef.current.projects.filter((project) => project.id !== id)
        dispatch({ type: 'projects', projects: remaining })
        if (stateRef.current.project?.id === id) {
          if (remaining[0]) dispatch({ type: 'open', project: remaining[0] })
          else {
            dispatch({ type: 'view', view: 'library' })
            dispatch({ type: 'modal', modal: null })
          }
        }
        toast('info', 'Project deleted')
      },
      closeProject: () => {
        dispatch({ type: 'view', view: 'library' })
        dispatch({ type: 'playing', playing: false })
      },
      saveNow: async () => {
        const project = stateRef.current.project
        if (!project) return
        dispatch({ type: 'saving' })
        try {
          const saved = await api.saveProject(project)
          dispatch({ type: 'saved', project: saved })
          void refreshProjects()
          toast('success', 'Project saved')
        } catch (error) {
          toast('error', 'Save failed', error instanceof Error ? error.message : undefined)
        }
      },
      importMedia: async () => {
        try {
          const assets = await api.importMedia()
          if (!assets.length) return
          mutate((project) => {
            project.assets.push(...assets)
            for (const asset of assets) {
              if (asset.kind === 'audio') continue
              appendAssetToTrack(project, asset.id, 'v1')
            }
          })
          toast('success', `${assets.length} ${assets.length === 1 ? 'file' : 'files'} imported`)
        } catch (error) {
          toast('error', 'Import failed', error instanceof Error ? error.message : undefined)
        }
      },
      dropFiles: async (files) => {
        const paths = files.map((file) => api.pathForFile(file)).filter(Boolean)
        if (!paths.length) {
          if (isDemo) {
            toast('info', 'Preview mode', 'File import becomes available in the desktop app.')
          }
          return
        }
        try {
          const assets = await api.importPaths(paths)
          if (!assets.length) {
            toast('error', 'Those files are not supported')
            return
          }
          mutate((project) => {
            project.assets.push(...assets)
            for (const asset of assets) appendAssetToTrack(project, asset.id, 'v1')
          })
          toast('success', `${assets.length} imported`)
        } catch (error) {
          toast('error', 'Import failed', error instanceof Error ? error.message : undefined)
        }
      },
      addAssetToTimeline: (asset) => {
        mutate((project) => {
          if (!project.assets.some((item) => item.id === asset.id)) project.assets.push(asset)
          appendAssetToTrack(project, asset.id, 'v1')
        })
      },
      setView: (view) => dispatch({ type: 'view', view }),
      setLeftPanel: (panel) => dispatch({ type: 'leftPanel', panel }),
      setInspectorTab: (tab) => dispatch({ type: 'inspectorTab', tab }),
      toggleSidebar: () => dispatch({ type: 'sidebar' }),
      toggleInspector: () => dispatch({ type: 'inspector' }),
      selectClip: (id) => dispatch({ type: 'selectClip', id }),
      selectCaption: (id) => dispatch({ type: 'selectCaption', id }),
      seek,
      step: (seconds) => {
        seek(stateRef.current.playhead + seconds)
      },
      togglePlay: () => {
        const project = stateRef.current.project
        if (!project) return
        const end = totalDuration(project)
        if (!stateRef.current.playing && stateRef.current.playhead >= end) seek(0)
        dispatch({ type: 'playing', playing: !stateRef.current.playing })
      },
      setPlaying: (playing) => dispatch({ type: 'playing', playing }),
      setZoom: (zoom) => dispatch({ type: 'zoom', zoom }),
      mutate,
      commitHistory: (previous) => dispatch({ type: 'commit', previous }),
      undo: () => dispatch({ type: 'undo' }),
      redo: () => dispatch({ type: 'redo' }),
      splitAtPlayhead: () => {
        const { project, playhead, selectedClipId } = stateRef.current
        if (!project) return
        const candidates = selectedClipId
          ? project.clips.filter((clip) => clip.id === selectedClipId)
          : project.clips.filter(
              (clip) => playhead > clip.start && playhead < clip.start + clip.duration
            )
        if (!candidates.length) {
          toast('info', 'Nothing to split here', 'Move the playhead inside a clip first.')
          return
        }
        mutate((draft) => {
          for (const clip of candidates) splitClipAt(draft, clip.id, playhead)
        })
      },
      deleteSelection: () => {
        const { project, selectedClipId, selectedCaptionId } = stateRef.current
        if (!project) return
        if (selectedClipId) {
          mutate((draft) => deleteClip(draft, selectedClipId))
          dispatch({ type: 'selectClip', id: null })
        } else if (selectedCaptionId) {
          mutate((draft) => deleteCaption(draft, selectedCaptionId))
          dispatch({ type: 'selectCaption', id: null })
        }
      },
      addCaptionAtPlayhead: () => {
        const { project, playhead } = stateRef.current
        if (!project) return
        let captionId = ''
        mutate((draft) => {
          const caption = addCaptionEdit(draft, playhead, playhead + 2.5, 'New caption')
          captionId = caption.id
        })
        dispatch({ type: 'selectCaption', id: captionId })
      },
      runJob: async (kind, targetAssetIdOverride, options) => {
        const { project } = stateRef.current
        if (!project) return
        const assetId = targetAssetIdOverride ?? targetAssetId
        const asset = project.assets.find((item) => item.id === assetId)
        if (!asset) {
          toast('error', 'Import media first', 'Analysis needs a file on disk.')
          return
        }
        if (!asset.path && !isDemo) {
          toast('error', 'This asset has no file on disk', 'Re-import the media to analyse it.')
          return
        }
        if (kind !== 'export' && !asset.path && !isDemo) return
        const request: RunRequest =
          kind === 'export'
            ? { kind: 'export', project, options: defaultExportOptions(project) }
            : kind === 'silence'
              ? { kind, project, assetId: asset.id, options: options?.silence }
              : kind === 'scenes'
                ? { kind, project, assetId: asset.id, options: options?.scenes }
                : { kind: 'transcribe', project, assetId: asset.id, options: options?.transcribe }
        dispatch({ type: 'playing', playing: false })
        try {
          await api.startJob(request)
        } catch (error) {
          toast('error', 'The engine could not start', error instanceof Error ? error.message : undefined)
        }
      },
      cancelJob: async () => {
        const job = stateRef.current.job
        if (!job) return
        await api.cancelJob(job.id)
      },
      dismissJob: () => dispatch({ type: 'clearJob' }),
      openExport: () => dispatch({ type: 'modal', modal: { kind: 'export' } }),
      startExport: async (options) => {
        const project = stateRef.current.project
        if (!project) return
        const suggested = `${project.name.replace(/[^\w\- ]+/g, '').trim() || 'clipfarm-export'}.mp4`
        const outputPath = await api.chooseExportPath(suggested)
        if (!outputPath) return
        dispatch({ type: 'modal', modal: null })
        dispatch({ type: 'playing', playing: false })
        try {
          await api.startJob({ kind: 'export', project, options: { ...options, outputPath } })
        } catch (error) {
          toast('error', 'Export could not start', error instanceof Error ? error.message : undefined)
        }
      },
      copy: async (text, label) => {
        await api.copyText(text)
        toast('success', `${label} copied`)
      },
      toast,
      dismissToast: (id) => dispatch({ type: 'dismissToast', id }),
      setModal: (modal) => dispatch({ type: 'modal', modal }),
      refreshEngine: async () => {
        const engine = await api.engineStatus().catch(() => null)
        if (engine) dispatch({ type: 'engine', engine })
      }
    }
  }, [targetAssetId, toast, refreshProjects])

  const value = useMemo<Studio>(() => ({ state, actions, targetAssetId, isDemo }), [state, actions, targetAssetId])

  return <StudioContext.Provider value={value}>{children}</StudioContext.Provider>
}

export function useStudio(): Studio {
  const studio = useContext(StudioContext)
  if (!studio) throw new Error('useStudio must be used inside StudioProvider')
  return studio
}

export function defaultExportOptions(project: Project): ExportOptions {
  return {
    outputPath: '',
    container: 'mp4',
    resolution: 'source',
    quality: 'balanced',
    burnCaptions: project.settings.captionsEnabled && project.captions.length > 0,
    fps: project.fps
  }
}
