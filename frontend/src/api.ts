// Client API TypeScript pour le backend Rust Todo2fast.

export interface Board {
  id: number
  name: string
  created_at: string
}

export interface List {
  id: number
  board_id: number
  title: string
  color: string | null
  position: number
}

export interface Todo {
  id: number
  board_id: number
  list_id: number | null
  title: string
  description: string
  done: boolean
  due_date: string | null
  position: number
  created_at: string
  updated_at: string
}

export interface Reaction {
  id: number
  comment_id: number
  author: string
  emoji: string
  created_at: string
}

export interface Comment {
  id: number
  todo_id: number
  author: string
  body: string
  created_at: string
  reactions: Reaction[]
}

export interface SuggestedTask {
  title: string
  due_date: string | null
}

export interface DocumentAnalysis {
  page_count: number
  char_count: number
  detected_dates: string[]
  suggested_tasks: SuggestedTask[]
  preview: string
}

export interface ImportResult {
  board: Board
  analysis: DocumentAnalysis
  created_count: number
  created: Todo[]
}

// ─── Helpers HTTP ─────────────────────────────────────────────────────────────

async function http<T>(method: string, url: string, body?: unknown): Promise<T> {
  const res = await fetch(url, {
    method,
    headers: body ? { 'Content-Type': 'application/json' } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  })
  if (!res.ok) {
    const text = await res.text().catch(() => '')
    throw new Error(`API ${res.status}: ${text || res.statusText}`)
  }
  // DELETE /api/lists/:id renvoie 204 sans corps.
  if (res.status === 204) return undefined as T
  return res.json() as Promise<T>
}

// ─── API ──────────────────────────────────────────────────────────────────────

export const api = {
  // Boards
  listBoards: () => http<Board[]>('GET', '/api/boards'),
  createBoard: (name: string) => http<Board>('POST', '/api/boards', { name }),
  getBoard: (id: number) => http<Board>('GET', `/api/boards/${id}`),
  deleteBoard: (id: number) => http<void>('DELETE', `/api/boards/${id}`),

  // Colonnes kanban
  listLists: (boardId: number) => http<List[]>('GET', `/api/boards/${boardId}/lists`),
  createList: (boardId: number, title: string, color?: string | null) =>
    http<List>('POST', `/api/boards/${boardId}/lists`, { title, ...(color ? { color } : {}) }),
  updateList: (id: number, patch: { title?: string; color?: string }) =>
    http<List>('PUT', `/api/lists/${id}`, patch),
  deleteList: (id: number) => http<void>('DELETE', `/api/lists/${id}`),

  // Todos
  listTodos: (boardId: number) => http<Todo[]>('GET', `/api/boards/${boardId}/todos`),
  createTodo: (
    boardId: number,
    title: string,
    opts?: { description?: string; due_date?: string | null; list_id?: number | null },
  ) =>
    http<Todo>('POST', `/api/boards/${boardId}/todos`, {
      title,
      description: opts?.description,
      due_date: opts?.due_date,
      list_id: opts?.list_id,
    }),
  getTodo: (id: number) => http<Todo>('GET', `/api/todos/${id}`),
  updateTodo: (
    id: number,
    patch: Partial<Pick<Todo, 'title' | 'description' | 'done' | 'due_date' | 'list_id'>>,
  ) => http<Todo>('PUT', `/api/todos/${id}`, patch),
  deleteTodo: (id: number) => http<void>('DELETE', `/api/todos/${id}`),

  // Commentaires
  listComments: (todoId: number) => http<Comment[]>('GET', `/api/todos/${todoId}/comments`),
  createComment: (todoId: number, author: string, body: string) =>
    http<Comment>('POST', `/api/todos/${todoId}/comments`, { author, body }),
  deleteComment: (id: number) => http<void>('DELETE', `/api/comments/${id}`),

  // Réactions
  addReaction: (commentId: number, author: string, emoji: string) =>
    http<Comment>('POST', `/api/comments/${commentId}/reactions`, { author, emoji }),
  removeReaction: (commentId: number, author: string, emoji: string) =>
    http<void>(
      'DELETE',
      `/api/comments/${commentId}/reactions?author=${encodeURIComponent(author)}&emoji=${encodeURIComponent(emoji)}`,
    ),

  // Import PDF — upload multipart + analyse
  async analyzePdf(file: File): Promise<DocumentAnalysis> {
    const form = new FormData()
    form.append('file', file)
    const res = await fetch('/api/documents', { method: 'POST', body: form })
    if (!res.ok) {
      const text = await res.text().catch(() => '')
      throw new Error(`API ${res.status}: ${text || res.statusText}`)
    }
    return res.json() as Promise<DocumentAnalysis>
  },

  // Import PDF directement dans un board (crée les tâches suggérées)
  async importPdf(boardId: number, file: File): Promise<ImportResult> {
    const form = new FormData()
    form.append('file', file)
    const res = await fetch(`/api/boards/${boardId}/import`, { method: 'POST', body: form })
    if (!res.ok) {
      const text = await res.text().catch(() => '')
      throw new Error(`API ${res.status}: ${text || res.statusText}`)
    }
    return res.json() as Promise<ImportResult>
  },
}
