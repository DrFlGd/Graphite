import type { SuggestionKeyDownProps, SuggestionProps } from '@tiptap/suggestion'
import { createRoot, type Root } from 'react-dom/client'
import type { ReactNode } from 'react'

export interface MenuItemView {
  key: string
  icon?: ReactNode
  label: string
  description?: string
  hint?: string
}

interface MenuState<I> {
  items: I[]
  selected: number
  query: string
}

/**
 * Shared popup renderer for the `/` block menu and the `[[` note picker.
 * Returns the `render()` object Tiptap's Suggestion plugin expects. Keeps a
 * single DOM element positioned at the caret and re-renders a React list
 * into it.
 */
export function suggestionMenu<I>(opts: {
  toView: (item: I) => MenuItemView
  emptyLabel: string
  className?: string
}) {
  return () => {
    let el: HTMLDivElement | null = null
    let root: Root | null = null
    let state: MenuState<I> = { items: [], selected: 0, query: '' }
    let command: (item: I) => void = () => {}

    const render = () => {
      if (!root) return
      root.render(
        <div className={`gr-menu ${opts.className ?? ''}`} role="listbox">
          {state.items.length === 0 ? (
            <div className="gr-menu__empty">{opts.emptyLabel}</div>
          ) : (
            state.items.map((item, i) => {
              const v = opts.toView(item)
              return (
                <button
                  key={v.key}
                  type="button"
                  role="option"
                  aria-selected={i === state.selected}
                  className={`gr-menu__item ${i === state.selected ? 'is-selected' : ''}`}
                  onMouseDown={(e) => {
                    e.preventDefault()
                    command(item)
                  }}
                  onMouseEnter={() => {
                    state = { ...state, selected: i }
                    render()
                  }}
                >
                  {v.icon && <span className="gr-menu__icon">{v.icon}</span>}
                  <span className="gr-menu__text">
                    <span className="gr-menu__label">{v.label}</span>
                    {v.description && <span className="gr-menu__desc">{v.description}</span>}
                  </span>
                  {v.hint && <span className="gr-menu__hint">{v.hint}</span>}
                </button>
              )
            })
          )}
        </div>,
      )
    }

    const position = (props: SuggestionProps<I>) => {
      if (!el) return
      const rect = props.clientRect?.()
      if (!rect) return
      const margin = 8
      const menuH = Math.min(320, el.offsetHeight || 320)
      const below = rect.bottom + 4
      const top = below + menuH > window.innerHeight - margin ? Math.max(margin, rect.top - menuH - 4) : below
      const left = Math.min(rect.left, window.innerWidth - (el.offsetWidth || 280) - margin)
      el.style.top = `${top}px`
      el.style.left = `${Math.max(margin, left)}px`
    }

    return {
      onStart: (props: SuggestionProps<I>) => {
        el = document.createElement('div')
        el.className = 'gr-menu-host'
        document.body.appendChild(el)
        root = createRoot(el)
        command = props.command
        state = { items: props.items, selected: 0, query: props.query }
        render()
        position(props)
      },
      onUpdate: (props: SuggestionProps<I>) => {
        command = props.command
        state = { items: props.items, selected: Math.min(state.selected, Math.max(0, props.items.length - 1)), query: props.query }
        render()
        position(props)
      },
      onKeyDown: ({ event }: SuggestionKeyDownProps) => {
        if (event.key === 'ArrowDown') {
          state = { ...state, selected: (state.selected + 1) % Math.max(1, state.items.length) }
          render()
          return true
        }
        if (event.key === 'ArrowUp') {
          state = { ...state, selected: (state.selected - 1 + Math.max(1, state.items.length)) % Math.max(1, state.items.length) }
          render()
          return true
        }
        if (event.key === 'Enter' || event.key === 'Tab') {
          const item = state.items[state.selected]
          if (item !== undefined) {
            command(item)
            return true
          }
          return false
        }
        if (event.key === 'Escape') {
          return false
        }
        return false
      },
      onExit: () => {
        root?.unmount()
        el?.remove()
        root = null
        el = null
      },
    }
  }
}
