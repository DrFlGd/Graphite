import React from 'react'
import ReactDOM from 'react-dom/client'
import '@graphite/ui/styles.css'
import './app.css'
import { App } from './App'
import { CaptureApp } from './CaptureApp'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    {window.location.hash === '#capture' ? <CaptureApp /> : <App />}
  </React.StrictMode>,
)
