import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { api, fileToBase64 } from './api'

/**
 * On a phone the dialog plugin hands back `content://` addresses, which the
 * core cannot open as paths. There the webview's own file picker is used
 * instead and the picked files' bytes are sent over; Android's picker grants
 * access to just the chosen files, so the app needs no storage permission.
 */
let phone = false

export function setPhonePicker(on: boolean) {
  phone = on
}

export interface Attached {
  /** Vault-relative path of the copy. */
  src: string
  /** The file's own name, for link text. */
  name: string
}

interface PickOptions {
  title: string
  multiple?: boolean
  images?: boolean
  /** Vault folder to copy into; the core's default attachments folder otherwise. */
  folder?: string
}

const IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg']

/** Let the user pick files and copy them into the vault. Resolves to nothing when they cancel. */
export async function pickAndAttach({ title, multiple = false, images = false, folder }: PickOptions): Promise<Attached[]> {
  if (phone) {
    const files = await pickInWebview(multiple, images ? 'image/*' : '')
    const out: Attached[] = []
    for (const f of files) out.push({ src: await api.attachData(f.name, await fileToBase64(f), folder), name: f.name })
    return out
  }
  const picked = await openDialog({ multiple, directory: false, title, filters: images ? [{ name: 'Images', extensions: IMAGE_EXTENSIONS }] : undefined })
  const paths = Array.isArray(picked) ? picked : picked ? [picked] : []
  const out: Attached[] = []
  for (const p of paths) out.push({ src: await api.attachFile(p, folder), name: p.split(/[\\/]/).pop() ?? p })
  return out
}

function pickInWebview(multiple: boolean, accept: string): Promise<File[]> {
  return new Promise((resolve) => {
    const input = document.createElement('input')
    input.type = 'file'
    input.multiple = multiple
    if (accept) input.accept = accept
    input.hidden = true
    // Kept in the page until the picker answers, so the webview doesn't drop it meanwhile.
    document.body.appendChild(input)
    const done = (files: File[]) => {
      input.remove()
      resolve(files)
    }
    input.addEventListener('change', () => done(Array.from(input.files ?? [])))
    input.addEventListener('cancel', () => done([]))
    input.click()
  })
}
