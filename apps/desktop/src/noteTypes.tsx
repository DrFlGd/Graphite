import type { CSSProperties } from 'react'
import {
  Archive, Atom, Battery, Bell, Bike, Bolt, Book, BookOpen, Bookmark, Bot, Box, Brain, Briefcase, Bug, Building2, CalendarDays, Camera, Car, CheckSquare, CircleCheck, CircleHelp, Clock, Cloud, Code, Coffee, Cpu, Database, DollarSign, Download, Dumbbell, FileText, Film,
  FlaskConical, Flag, Flame, Folder, Gamepad2, Gift, Globe, GraduationCap, Hammer, Heart, HeartPulse, House, Inbox, Info, Key, Leaf, Lightbulb, Link, ListFilter, Lock, Mail, Map as MapIcon, MapPin, MessageCircle, Microscope, Moon, Music, Network, NotebookPen, Package, Paintbrush,
  Palette, Pen, Phone, Pin, Plane, Printer, Puzzle, Rocket, Scale, Server, Shield, ShoppingCart, Smile, Sparkles, Sprout, SquareKanban, Star, Sun, Target, Terminal, TreePine, TriangleAlert, Trophy, User, Users, Utensils, Wallet, Wrench, Zap, type LucideIcon,
} from 'lucide-react'

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

/**
 * Icons a note can name in its `icon` front-matter (A8), by their Lucide
 * names; `lucide-star` works too. Anything else that is not a plain word,
 * such as an emoji, is shown as it is.
 */
export const NOTE_ICONS: Record<string, LucideIcon> = {
  star: Star, heart: Heart, flag: Flag, bookmark: Bookmark, pin: Pin, lightbulb: Lightbulb, rocket: Rocket, target: Target, trophy: Trophy, sparkles: Sparkles, zap: Zap, flame: Flame,
  leaf: Leaf, sprout: Sprout, 'tree-pine': TreePine, sun: Sun, moon: Moon, cloud: Cloud, house: House, briefcase: Briefcase, 'building-2': Building2, 'graduation-cap': GraduationCap, book: Book, 'book-open': BookOpen,
  'notebook-pen': NotebookPen, pen: Pen, paintbrush: Paintbrush, palette: Palette, code: Code, terminal: Terminal, cpu: Cpu, server: Server, database: Database, network: Network, bot: Bot, bug: Bug,
  wrench: Wrench, hammer: Hammer, bolt: Bolt, battery: Battery, printer: Printer, 'flask-conical': FlaskConical, microscope: Microscope, atom: Atom, brain: Brain, puzzle: Puzzle, camera: Camera, music: Music,
  film: Film, 'gamepad-2': Gamepad2, 'map-pin': MapPin, globe: Globe, plane: Plane, car: Car, bike: Bike, dumbbell: Dumbbell, 'heart-pulse': HeartPulse, utensils: Utensils, coffee: Coffee, 'shopping-cart': ShoppingCart,
  wallet: Wallet, 'dollar-sign': DollarSign, calendar: CalendarDays, clock: Clock, bell: Bell, lock: Lock, key: Key, shield: Shield, users: Users, user: User, smile: Smile, 'message-circle': MessageCircle,
  mail: Mail, phone: Phone, gift: Gift, package: Package, box: Box, archive: Archive, folder: Folder, 'file-text': FileText, link: Link, 'circle-check': CircleCheck, 'triangle-alert': TriangleAlert, info: Info,
}

/** Colours a note can name in its `color` front-matter; `#rrggbb` works too. */
export const NOTE_COLOURS: Record<string, string> = {
  red: '#d2384f',
  orange: '#e5673d',
  yellow: '#d99a00',
  green: '#2e9e5b',
  teal: '#14a0a0',
  blue: '#3b6ef5',
  purple: '#8a5cf6',
  pink: '#d6409f',
  brown: '#9c6f4e',
  grey: '#8b8e96',
}

/** A `color` value as CSS, or undefined when it is not one Graphite knows. */
export function noteColour(color?: string | null): string | undefined {
  const c = color?.trim().toLowerCase()
  if (!c) return undefined
  if (/^#([0-9a-f]{3}|[0-9a-f]{6})$/.test(c)) return c
  return NOTE_COLOURS[c === 'gray' ? 'grey' : c]
}

/** What a note's `icon` names: a Lucide icon, some text (an emoji), or nothing usable. */
export function noteIcon(icon?: string | null): { Icon: LucideIcon } | { text: string } | null {
  const v = icon?.trim()
  if (!v) return null
  const name = v.toLowerCase().replace(/^lucide-/, '')
  if (NOTE_ICONS[name]) return { Icon: NOTE_ICONS[name] }
  // An unknown icon name reads as a typo, not as something to show.
  if (/^[a-z0-9-]+$/i.test(v)) return null
  return { text: [...v].slice(0, 2).join('') }
}

/** The look a note's front-matter gives it, from a summary (`icon`, `color`) or a full note's properties. */
export function lookOf(n: { icon?: string | null; color?: string | null; properties?: Record<string, unknown> } | null | undefined): { icon?: string; color?: string } {
  if (!n) return {}
  const p = n.properties
  const text = (v: unknown) => (typeof v === 'string' && v.trim() ? v.trim() : undefined)
  return { icon: text(n.icon) ?? text(p?.icon), color: text(n.color) ?? text(p?.color) }
}

/**
 * The note's own icon and colour when its front-matter sets them, else its
 * type's icon in the type's colour; a plain page for ordinary notes.
 */
export function TypeIcon({ type, icon, color, size = 14 }: { type?: string | null; icon?: string | null; color?: string | null; size?: number }) {
  const own = noteIcon(icon)
  const colour = noteColour(color) ?? typeColour(type)
  if (own && 'text' in own) {
    return (
      <span className="type-icon type-icon--text" style={{ fontSize: size, width: size + 2 }} aria-hidden>
        {own.text}
      </span>
    )
  }
  const Icon = own?.Icon ?? ((type && TYPE_ICONS[type]) || FileText)
  return <Icon size={size} className="type-icon" color={colour ?? 'currentColor'} aria-hidden />
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
