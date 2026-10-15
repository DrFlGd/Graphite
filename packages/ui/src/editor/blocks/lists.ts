import { List, ListOrdered, ListChecks, MessageCircleQuestionMark } from 'lucide-react'
import TaskList from '@tiptap/extension-task-list'
import { GraphiteTaskItem } from './taskItem'
import type { BlockDef } from './types'

export const bulletList: BlockDef = {
  id: 'bulletList',
  label: 'Bullet list',
  description: 'Unordered list',
  icon: List,
  group: 'list',
  keywords: ['ul', 'bullets'],
  shortcut: 'Ctrl+Shift+8',
  insert: (editor) => editor.chain().focus().toggleBulletList().run(),
  isActive: (editor) => editor.isActive('bulletList'),
}

export const orderedList: BlockDef = {
  id: 'orderedList',
  label: 'Numbered list',
  description: 'Ordered list',
  icon: ListOrdered,
  group: 'list',
  keywords: ['ol', 'numbers', 'steps'],
  shortcut: 'Ctrl+Shift+7',
  insert: (editor) => editor.chain().focus().toggleOrderedList().run(),
  isActive: (editor) => editor.isActive('orderedList'),
}

export const taskList: BlockDef = {
  id: 'taskList',
  label: 'Task list',
  description: 'Checkboxes; each item is a task in the project',
  icon: ListChecks,
  group: 'list',
  keywords: ['todo', 'checkbox', 'tasks', 'checklist'],
  shortcut: 'Ctrl+Shift+9',
  extensions: [TaskList, GraphiteTaskItem.configure({ nested: true })],
  insert: (editor) => editor.chain().focus().toggleTaskList().run(),
  isActive: (editor) => editor.isActive('taskList'),
}

export const gap: BlockDef = {
  id: 'gap',
  label: 'Open question',
  description: 'Something to find out: a `- [?]` line that tracks until answered',
  icon: MessageCircleQuestionMark,
  group: 'list',
  keywords: ['gap', 'question', 'unknown', 'find out', 'research'],
  extensions: [TaskList, GraphiteTaskItem.configure({ nested: true })],
  insert: (editor) => editor.chain().focus().insertGap().run(),
  isActive: (editor) => editor.isActive('taskItem', { state: 'gap' }),
  toolbar: false,
}
