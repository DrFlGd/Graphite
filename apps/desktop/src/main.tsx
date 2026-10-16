import React from 'react'
import ReactDOM from 'react-dom/client'
import '@graphite/ui/styles.css'
import './app.css'
import { App } from './App'
import { CaptureApp } from './CaptureApp'
import { MobileApp } from './MobileApp'
import { api } from './api'
import { setPhonePicker } from './filePicker'

const root = ReactDOM.createRoot(document.getElementById('root')!)

function render(platform: string) {
  const phone = platform === 'android' || platform === 'ios'
  if (phone) document.documentElement.classList.add('is-phone')
  setPhonePicker(phone)
  root.render(<React.StrictMode>{phone ? <MobileApp /> : window.location.hash === '#capture' ? <CaptureApp /> : <App />}</React.StrictMode>)
}

api
  .platform()
  .then(render)
  .catch(() => render('unknown'))
