import type { CSSProperties } from 'react'
import { BookOpen, CalendarDays, CheckSquare, CircleHelp, Download, FileText, Inbox, Lightbulb, ListFilter, Map as MapIcon, Scale, SquareKanban, User, Users, type LucideIcon } from 'lucide-react'

/**
 * One colour and one icon per note type, used wherever a note shows up
 * (sidebar, tabs, search, the side panel, the graph) so a type reads the
 * same everywhere. Types not listed here are neutral.
 */
export const TYPE_COLOURS: Record<string, string> = {
  project: '#3b6ef5',
  area: '#2e9e5b',
  idea: '#d99a00',
  daily: '#14a0a0',
  capture: '#8b8e96',
  source: '#8a5cf6',
  reading: '#6d8c2f',
  decision: '#5b7a8a',
  meeting: '#b5651d',
  query: '#d6409f',
  person: '#9c6f4e',
  task: '#e5673d',
  gap: '#d2384f',
}

const TYPE_ICONS: Record<string, LucideIcon> = {
  project: SquareKanban,
  area: MapIcon,
  idea: Lightbulb,
  daily: CalendarDays,
  capture: Inbox,
  source: Download,
  reading: BookOpen,
  decision: Scale,
  meeting: Users,
  query: ListFilter,
  person: User,
  task: CheckSquare,
  gap: CircleHelp,
}

export const typeColour = (type?: string | null): string | undefined => (type ? TYPE_COLOURS[type] : undefined)

/** The type's icon in the type's colour; a plain page for ordinary notes. */
export function TypeIcon({ type, size = 14 }: { type?: string | null; size?: number }) {
  const Icon = (type && TYPE_ICONS[type]) || FileText
  return <Icon size={size} className="type-icon" color={typeColour(type) ?? 'currentColor'} aria-hidden />
}

/** A small label naming the type, tinted with its colour. */
export function TypeChip({ type, icon = false, className = '' }: { type: string; icon?: boolean; className?: string }) {
  const colour = typeColour(type)
  return (
    <span className={`type-chip ${colour ? '' : 'type-chip--plain'} ${className}`} style={colour ? ({ '--t': colour } as CSSProperties) : undefined}>
      {icon && <TypeIcon type={type} size={11} />}
      {type}
    </span>
  )
}

/** Same rule as the core's `dated_daily_title`: a renamed daily note keeps its date, as `Name (YYYY-MM-DD)`. */
export function datedDailyTitle(title: string, path: string): string {
  const stem = (path.split('/').pop() ?? path).replace(/\.md$/, '')
  const t = title.trim()
  if (!/^\d{4}-\d{2}-\d{2}$/.test(stem) || t.includes(stem)) return t
  return t ? `${t} (${stem})` : stem
}
