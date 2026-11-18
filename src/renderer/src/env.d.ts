/// <reference types="vite/client" />

import type { ClipfarmApi } from '@shared/types'

declare global {
  interface Window {
    clipfarm?: ClipfarmApi
  }
}

export {}
