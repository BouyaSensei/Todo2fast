// Client API TypeScript pour le backend Rust Todo2fast.

export interface Board {
  id: number
  name: string
  created_at: string
}

export interface Todo {
  id: number
  board_id: number
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
  return res.json() as Promise<T>
}

// ─── Boards ───────────────────────────────────────────────────────────────────

export const api = {
  // Boards
  listBoards: () => http<Board[]>('GET', '/api/boards'),
  createBoard: (name: string) =>
    http<Board>('POST', '/api/boards', { name }),
  getBoard: (id: number) => http<Board>('GET', `/api/boards/${id}`),
  deleteBoard: (id: number) => http<void>('DELETE', `/api/boards/${id}`),

  // Todos
  listTodos: (boardId: number) => http<Todo[]>('GET', `/api/boards/${boardId}/todos`),
  createTodo: (boardId: number, title: string, description?: string) =>
    http<Todo>('POST', `/api/boards/${boardId}/todos`, { title, description }),
  getTodo: (id: number) => http<Todo>('GET', `/api/todos/${id}`),
  updateTodo: (id: number, patch: Partial<Pick<Todo, 'title' | 'description' | 'done'>>) =>
    http<Todo>('PUT', `/api/todos/${id}`, patch),
  deleteTodo: (id: number) => http<void>('DELETE', `/api/todos/${id}`),

  // Comments
  listComments: (todoId: number) => http<Comment[]>('GET', `/api/todos/${todoId}/comments`),
  createComment: (todoId: number, author: string, body: string) =>
    http<Comment>('POST', `/api/todos/${todoId}/comments`, { author, body }),
  deleteComment: (id: number) => http<void>('DELETE', `/api/comments/${id}`),

  // Reactions
  addReaction: (commentId: number, author: string, emoji: string) =>
    http<Comment>('POST', `/api/comments/${commentId}/reactions`, { author, emoji }),
  removeReaction: (commentId: number, author: string, emoji: string) =>
    http<void>(
      'DELETE',
      `/api/comments/${commentId}/reactions?author=${encodeURIComponent(author)}&emoji=${encodeURIComponent(emoji)}`,
    ),
}
