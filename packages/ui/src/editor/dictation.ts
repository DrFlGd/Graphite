/**
 * Dictation (B11): records the microphone as 16 kHz mono 16-bit PCM for the
 * host to recognise on the device. Nothing here leaves the app.
 */

export const DICTATION_RATE = 16_000

/** What the host provides for dictation. */
export interface DictationHost {
  /** Get ready to dictate (the speech model may need downloading); false if not now. */
  prepare: () => Promise<boolean>
  /** The words in a recording. */
  transcribe: (pcm: Int16Array) => Promise<string>
  onError?: (message: string) => void
}

export interface Recording {
  /** Stop and return the audio. */
  stop: () => Promise<Int16Array>
  /** Stop and drop the audio. */
  cancel: () => void
}

export function concatAudio(chunks: Float32Array[]): Float32Array {
  const out = new Float32Array(chunks.reduce((n, c) => n + c.length, 0))
  let at = 0
  for (const c of chunks) {
    out.set(c, at)
    at += c.length
  }
  return out
}

/** Down-sample by averaging each output sample's span of input (enough for speech). */
export function resampleAudio(input: Float32Array, from: number, to: number): Float32Array {
  if (from === to || input.length === 0) return input
  const ratio = from / to
  const out = new Float32Array(Math.floor(input.length / ratio))
  for (let i = 0; i < out.length; i++) {
    const start = Math.floor(i * ratio)
    const end = Math.max(start + 1, Math.min(input.length, Math.floor((i + 1) * ratio)))
    let sum = 0
    for (let j = start; j < end; j++) sum += input[j]
    out[i] = sum / (end - start)
  }
  return out
}

export function toPcm16(input: Float32Array): Int16Array {
  const out = new Int16Array(input.length)
  for (let i = 0; i < input.length; i++) {
    const s = Math.max(-1, Math.min(1, input[i]))
    out[i] = s < 0 ? Math.round(s * 32768) : Math.round(s * 32767)
  }
  return out
}

/** Start recording; `onLevel` gets the input level (0 to 1) a few times a second. */
export async function startRecording(onLevel?: (level: number) => void): Promise<Recording> {
  if (!navigator.mediaDevices?.getUserMedia) throw new Error('No microphone is available here.')
  const stream = await navigator.mediaDevices.getUserMedia({
    audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true, autoGainControl: true },
  })
  let ctx: AudioContext
  try {
    ctx = new AudioContext({ sampleRate: DICTATION_RATE })
  } catch {
    ctx = new AudioContext()
  }
  const source = ctx.createMediaStreamSource(stream)
  // ScriptProcessor is deprecated but works in every webview Graphite runs
  // in, and an AudioWorklet module would need its own script URL.
  const proc = ctx.createScriptProcessor(2048, 1, 1)
  const mute = ctx.createGain()
  mute.gain.value = 0
  const chunks: Float32Array[] = []
  proc.onaudioprocess = (e) => {
    const data = e.inputBuffer.getChannelData(0)
    chunks.push(new Float32Array(data))
    if (onLevel) {
      let sum = 0
      for (let i = 0; i < data.length; i++) sum += data[i] * data[i]
      onLevel(Math.min(1, Math.sqrt(sum / data.length) * 5))
    }
  }
  source.connect(proc)
  proc.connect(mute)
  mute.connect(ctx.destination)
  if (ctx.state === 'suspended') await ctx.resume()
  const close = () => {
    proc.onaudioprocess = null
    source.disconnect()
    proc.disconnect()
    mute.disconnect()
    stream.getTracks().forEach((t) => t.stop())
    void ctx.close()
  }
  return {
    stop: async () => {
      const rate = ctx.sampleRate
      close()
      return toPcm16(resampleAudio(concatAudio(chunks), rate, DICTATION_RATE))
    },
    cancel: close,
  }
}

/** The text to insert after `before` (the character left of the cursor): a space between words. */
export function spacedTranscript(before: string, text: string): string {
  const t = text.trim()
  if (!t) return ''
  return before && !/\s/.test(before) ? ` ${t}` : t
}
