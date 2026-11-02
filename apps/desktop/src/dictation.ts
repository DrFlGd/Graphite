import type { DictationHost } from '@graphite/ui'
import { api } from './api'

/**
 * Dictation on this computer (B11). The first time, it offers to download
 * the speech model; the download shows in the jobs panel, and Dictate works
 * once it is done.
 */
export function dictationHost(onError: (message: string) => void): DictationHost {
  return {
    prepare: async () => {
      const status = await api.dictationStatus()
      if (!status.available) {
        onError('Dictation is not part of this build. On a phone, use the microphone on your keyboard.')
        return false
      }
      if (status.ready) return true
      const ok = window.confirm(
        `Dictation turns speech into text on this computer; nothing is sent anywhere. It needs a speech model (${status.model_mb} MB), downloaded once from Hugging Face. Download it now?`,
      )
      if (ok) await api.startInstallDictationModel()
      return false
    },
    transcribe: (pcm) => api.transcribe(pcm),
    onError,
  }
}
