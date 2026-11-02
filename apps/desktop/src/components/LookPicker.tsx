import { useEffect, useRef, useState, type CSSProperties } from 'react'
import { NOTE_COLOURS, NOTE_ICONS, TypeIcon, noteColour, noteIcon } from '../noteTypes'

interface Props {
  type: string
  icon?: string
  color?: string
  /** `null` removes the key from the front-matter. */
  onChange: (look: { icon?: string | null; color?: string | null }) => void
}

/**
 * The note's icon, shown left of its title. Clicking it picks an icon (or
 * any emoji) and a colour, kept in the note's `icon` and `color`
 * front-matter (A8).
 */
export function LookPicker({ type, icon, color, onChange }: Props) {
  const [open, setOpen] = useState(false)
  const [text, setText] = useState('')
  const root = useRef<HTMLDivElement>(null)
  const own = Boolean(icon || color)

  useEffect(() => {
    if (!open) return
    setText(icon && !NOTE_ICONS[icon.toLowerCase().replace(/^lucide-/, '')] ? icon : '')
    const onDown = (e: MouseEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false)
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false)
    }
    window.addEventListener('mousedown', onDown)
    window.addEventListener('keydown', onKey)
    return () => {
      window.removeEventListener('mousedown', onDown)
      window.removeEventListener('keydown', onKey)
    }
  }, [open, icon])

  const current = icon?.toLowerCase().replace(/^lucide-/, '')
  const commitText = () => {
    const t = text.trim()
    if (t && noteIcon(t)) onChange({ icon: t })
  }

  return (
    <div className="look" ref={root}>
      <button type="button" className={`look__button ${own ? 'is-set' : ''}`} title="Icon and colour" aria-label="Icon and colour" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <TypeIcon type={type} icon={icon} color={color} size={22} />
      </button>
      {open && (
        <div className="look__menu" role="dialog" aria-label="Icon and colour">
          <div className="look__icons">
            {Object.entries(NOTE_ICONS).map(([name, Icon]) => (
              <button key={name} type="button" className={`look__icon ${current === name ? 'is-current' : ''}`} title={name} aria-label={name} onClick={() => onChange({ icon: name })}>
                <Icon size={16} color={noteColour(color) ?? 'currentColor'} />
              </button>
            ))}
          </div>
          <form
            className="look__text"
            onSubmit={(e) => {
              e.preventDefault()
              commitText()
            }}
          >
            <input value={text} onChange={(e) => setText(e.target.value)} placeholder="Or type an emoji" aria-label="Emoji" maxLength={8} />
            <button type="submit" className="look__small" disabled={!text.trim() || !noteIcon(text)}>
              Use
            </button>
          </form>
          <div className="look__colours">
            <button type="button" className={`look__swatch look__swatch--none ${color ? '' : 'is-current'}`} title="Type colour" aria-label="Type colour" onClick={() => onChange({ color: null })} />
            {Object.entries(NOTE_COLOURS).map(([name, hex]) => (
              <button key={name} type="button" className={`look__swatch ${color?.toLowerCase() === name ? 'is-current' : ''}`} style={{ '--c': hex } as CSSProperties} title={name} aria-label={name} onClick={() => onChange({ color: name })} />
            ))}
          </div>
          {own && (
            <button
              type="button"
              className="look__small look__reset"
              onClick={() => {
                onChange({ icon: null, color: null })
                setOpen(false)
              }}
            >
              Use the type's icon and colour
            </button>
          )}
        </div>
      )}
    </div>
  )
}
