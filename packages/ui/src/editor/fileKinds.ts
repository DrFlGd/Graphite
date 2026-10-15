/** Which inline preview a non-image file gets; shared by the editor and the host app. */

export const VIDEO_EXTS = ['mp4', 'm4v', 'mov', 'webm', 'mkv', 'ogv']
export const AUDIO_EXTS = ['mp3', 'm4a', 'wav', 'flac', 'ogg', 'oga', 'aac', 'opus']
export const MODEL_EXTS = ['gltf', 'glb', 'obj', 'stl', 'ply', '3mf', 'step', 'stp', 'iges', 'igs', 'fbx']

export const extOf = (src: string): string => src.split(/[?#]/)[0].split('.').pop()?.toLowerCase() ?? ''

/** The kind of inline preview a file embed gets. */
export type EmbedKind = 'video' | 'audio' | 'pdf' | 'model' | 'file'
export function embedKind(src: string): EmbedKind {
  const ext = extOf(src)
  if (VIDEO_EXTS.includes(ext)) return 'video'
  if (AUDIO_EXTS.includes(ext)) return 'audio'
  if (ext === 'pdf') return 'pdf'
  if (MODEL_EXTS.includes(ext)) return 'model'
  return 'file'
}

/** Files that `insertAttachments` embeds (`![name](path)`) rather than links. */
export const isMediaSrc = (src: string): boolean => embedKind(src) !== 'file'
