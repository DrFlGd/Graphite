import { afterEach, describe, expect, it, vi } from 'vitest'
import { open } from '@tauri-apps/plugin-dialog'
import { api } from './api'
import { pickAndAttach, setPhonePicker } from './filePicker'

vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('./api', () => ({
  api: {
    attachData: vi.fn(async (name: string, _data: string, folder?: string) => `${folder ?? 'attachments'}/${name}`),
    attachFile: vi.fn(async (src: string, folder?: string) => `${folder ?? 'attachments'}/${src.split('/').pop()}`),
  },
  fileToBase64: vi.fn(async () => 'AAAA'),
}))

/** A stand-in for the webview's `<input type="file">`: clicking it "picks" `files`, or cancels when null. */
function fakeInput(files: { name: string }[] | null) {
  const handlers: Record<string, () => void> = {}
  const input = {
    type: '',
    hidden: false,
    multiple: false,
    accept: '',
    files,
    addEventListener: (ev: string, fn: () => void) => (handlers[ev] = fn),
    click: () => (files ? handlers.change() : handlers.cancel()),
    remove: vi.fn(),
  }
  vi.stubGlobal('document', { createElement: () => input, body: { appendChild: vi.fn() } })
  return input
}

afterEach(() => {
  vi.unstubAllGlobals()
  vi.clearAllMocks()
  setPhonePicker(false)
})

describe('pickAndAttach', () => {
  it('on a phone sends the picked files’ bytes instead of their content:// address', async () => {
    setPhonePicker(true)
    const input = fakeInput([{ name: 'photo.jpg' }, { name: 'spec.pdf' }])
    const added = await pickAndAttach({ title: 'Add files', multiple: true, folder: 'attachments/Robot' })
    expect(input.multiple).toBe(true)
    expect(input.remove).toHaveBeenCalled()
    expect(api.attachData).toHaveBeenCalledWith('photo.jpg', 'AAAA', 'attachments/Robot')
    expect(added).toEqual([
      { src: 'attachments/Robot/photo.jpg', name: 'photo.jpg' },
      { src: 'attachments/Robot/spec.pdf', name: 'spec.pdf' },
    ])
    expect(open).not.toHaveBeenCalled()
    expect(api.attachFile).not.toHaveBeenCalled()
  })

  it('on a phone limits the picker to images when asked', async () => {
    setPhonePicker(true)
    const input = fakeInput([{ name: 'a.png' }])
    await pickAndAttach({ title: 'Insert image', images: true })
    expect(input.accept).toBe('image/*')
  })

  it('adds nothing when the picker is cancelled', async () => {
    setPhonePicker(true)
    fakeInput(null)
    expect(await pickAndAttach({ title: 'Add files' })).toEqual([])
    expect(api.attachData).not.toHaveBeenCalled()
  })

  it('on a desktop copies the chosen paths', async () => {
    vi.mocked(open).mockResolvedValueOnce(['/home/me/mount.stl'] as never)
    const added = await pickAndAttach({ title: 'Add files', multiple: true })
    expect(api.attachFile).toHaveBeenCalledWith('/home/me/mount.stl', undefined)
    expect(added).toEqual([{ src: 'attachments/mount.stl', name: 'mount.stl' }])
  })
})
