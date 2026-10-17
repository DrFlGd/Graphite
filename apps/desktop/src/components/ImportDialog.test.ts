import { describe, expect, it } from 'vitest'
import type { Tools } from '../api'
import { missingToolFor, videoToolsNote } from './ImportDialog'

const tools = (over: Partial<Tools>): Tools => ({ ffmpeg: false, ffprobe: false, pdfinfo: false, pdftotext: false, pdftoppm: false, git: false, yt_dlp: false, deno: false, browser: false, ...over })

describe('missingToolFor', () => {
  it('lets GitHub repositories through without git', () => {
    expect(missingToolFor('github', 'https://github.com/acme/widget', tools({}))).toBeNull()
    expect(missingToolFor('github', 'https://gitlab.com/acme/widget', tools({}))).toBe('git')
    expect(missingToolFor('git', 'git@github.com:acme/widget.git', tools({}))).toBe('git')
    expect(missingToolFor('git', 'https://example.com/x.git', tools({ git: true }))).toBeNull()
  })
  it('asks for yt-dlp for videos only', () => {
    expect(missingToolFor('video', 'https://youtu.be/x', tools({}))).toBe('yt-dlp')
    expect(missingToolFor('video', 'https://youtu.be/x', tools({ yt_dlp: true }))).toBeNull()
    expect(missingToolFor('page', 'https://example.com', tools({}))).toBeNull()
  })
  it('says what the video tools button downloads', () => {
    expect(videoToolsNote(['yt-dlp', 'deno', 'ffmpeg'])).toBe('Gets yt-dlp, Deno and ffmpeg from their official releases, about 285 MB.')
    expect(videoToolsNote(['deno'])).toBe('Gets Deno from its official releases, about 45 MB.')
    expect(videoToolsNote([])).toMatch(/newest yt-dlp/)
  })
})
