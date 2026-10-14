import { List, ListOrdered, ListChecks } from 'lucide-react'
import TaskList from '@tiptap/extension-task-list'
import TaskItem from '@tiptap/extension-task-item'
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
  extensions: [TaskList, TaskItem.configure({ nested: true })],
  insert: (editor) => editor.chain().focus().toggleTaskList().run(),
  isActive: (editor) => editor.isActive('taskList'),
}
