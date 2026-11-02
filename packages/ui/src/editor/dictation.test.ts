import { describe, expect, it } from 'vitest'
import { concatAudio, resampleAudio, spacedTranscript, toPcm16 } from './dictation'

describe('dictation audio', () => {
  it('joins chunks, averages down to 16 kHz and converts to 16-bit', () => {
    const joined = concatAudio([new Float32Array([0.1, 0.2]), new Float32Array([0.3])])
    expect(Array.from(joined).map((x) => Math.round(x * 10))).toEqual([1, 2, 3])
    const down = resampleAudio(new Float32Array([0, 0.3, 0.6, 1, 1, 1]), 48_000, 16_000)
    expect(down.length).toBe(2)
    expect(down[0]).toBeCloseTo(0.3)
    expect(down[1]).toBeCloseTo(1)
    expect(Array.from(toPcm16(new Float32Array([-1, 0, 1, 2])))).toEqual([-32768, 0, 32767, 32767])
  })

  it('spaces the words from what is already there', () => {
    expect(spacedTranscript('d', ' Hello there. ')).toBe(' Hello there.')
    expect(spacedTranscript(' ', 'Hello')).toBe('Hello')
    expect(spacedTranscript('', 'Hello')).toBe('Hello')
    expect(spacedTranscript('d', '  ')).toBe('')
  })
})
