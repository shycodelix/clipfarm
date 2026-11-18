import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import { StudioProvider } from './store'
import './styles.css'

const container = document.getElementById('root')
if (!container) throw new Error('clipfarm could not find its root element')

createRoot(container).render(
  <StrictMode>
    <StudioProvider>
      <App />
    </StudioProvider>
  </StrictMode>
)
