import type { ClipfarmApi } from '@shared/types'
import { createDemoApi } from './demo'

/**
 * The desktop app exposes a typed bridge from the preload script. When the same
 * UI is opened in a browser (`npm run dev:web`) we swap in a demo backend with
 * seeded projects and simulated analysis so design work never needs Electron.
 */
export const isDesktop = typeof window !== 'undefined' && Boolean(window.clipfarm)

export const api: ClipfarmApi = window.clipfarm ?? createDemoApi()

export const isDemo = !isDesktop
