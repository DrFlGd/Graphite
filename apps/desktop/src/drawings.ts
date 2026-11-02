import { encodeSrc, type BlockContext } from '@graphite/ui'
import { api, decodePath } from './api'

/** Editor callbacks for drawings: new ones go in `attachments/drawings/`, embeds read the file for their picture. */
export const drawingContext: Pick<BlockContext, 'newDrawing' | 'readDrawing'> = {
  newDrawing: async () => encodeSrc(await api.newDrawing()),
  readDrawing: (src) => api.readDrawing(decodePath(src)),
}
