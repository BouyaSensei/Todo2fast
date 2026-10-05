import { useCallback, useEffect, useState } from 'react'
import { api, type Board, type Comment, type Todo } from './api'
import './App.css'

const EMOJIS = ['👍', '🎉', '❤️', '🚀', '✅']

function App() {
  const [boards, setBoards] = useState<Board[]>([])
  const [activeBoard, setActiveBoard] = useState<Board | null>(null)
  const [todos, setTodos] = useState<Todo[]>([])
  const [selectedTodo, setSelectedTodo] = useState<Todo | null>(null)
  const [comments, setComments] = useState<Comment[]>([])
  const [newBoardName, setNewBoardName] = useState('')
  const [newTodoTitle, setNewTodoTitle] = useState('')
  const [newCommentBody, setNewCommentBody] = useState('')
  const [author, setAuthor] = useState(() => localStorage.getItem('t2f_author') || 'vous')
  const [error, setError] = useState<string | null>(null)

  const refreshBoards = useCallback(async () => {
    try {
      const b = await api.listBoards()
      setBoards(b)
      return b
    } catch (e) {
      setError(String(e))
      return []
    }
  }, [])

  const loadTodos = useCallback(async (board: Board) => {
    try {
      const t = await api.listTodos(board.id)
      setTodos(t.sort((a, b) => a.position - b.position))
    } catch (e) {
      setError(String(e))
    }
  }, [])

  const loadComments = useCallback(async (todo: Todo) => {
    try {
      const c = await api.listComments(todo.id)
      setComments(c)
    } catch (e) {
      setError(String(e))
    }
  }, [])

  useEffect(() => {
    localStorage.setItem('t2f_author', author)
  }, [author])

  // Initial load
  useEffect(() => {
    refreshBoards().then((b) => {
      if (b.length > 0 && !activeBoard) selectBoard(b[0])
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  async function selectBoard(board: Board) {
    setActiveBoard(board)
    setSelectedTodo(null)
    setComments([])
    await loadTodos(board)
  }

  async function handleCreateBoard(e: React.FormEvent) {
    e.preventDefault()
    if (!newBoardName.trim()) return
    const b = await api.createBoard(newBoardName.trim())
    setNewBoardName('')
    await refreshBoards()
    await selectBoard(b)
  }

  async function handleCreateTodo(e: React.FormEvent) {
    e.preventDefault()
    if (!activeBoard || !newTodoTitle.trim()) return
    await api.createTodo(activeBoard.id, newTodoTitle.trim())
    setNewTodoTitle('')
    await loadTodos(activeBoard)
  }

  async function toggleDone(todo: Todo) {
    if (!activeBoard) return
    const updated = await api.updateTodo(todo.id, { done: !todo.done })
    setTodos((prev) => prev.map((t) => (t.id === todo.id ? updated : t)))
    if (selectedTodo?.id === todo.id) setSelectedTodo(updated)
  }

  async function handleSelectTodo(todo: Todo) {
    setSelectedTodo(todo)
    await loadComments(todo)
  }

  async function handleDeleteTodo(id: number) {
    if (!activeBoard) return
    await api.deleteTodo(id)
    setSelectedTodo(null)
    setComments([])
    await loadTodos(activeBoard)
  }

  async function handleCreateComment(e: React.FormEvent) {
    e.preventDefault()
    if (!selectedTodo || !newCommentBody.trim()) return
    await api.createComment(selectedTodo.id, author, newCommentBody.trim())
    setNewCommentBody('')
    await loadComments(selectedTodo)
  }

  async function handleReaction(comment: Comment, emoji: string) {
    if (!selectedTodo) return
    const has = comment.reactions.some((r) => r.author === author && r.emoji === emoji)
    if (has) {
      await api.removeReaction(comment.id, author, emoji)
    } else {
      await api.addReaction(comment.id, author, emoji)
    }
    await loadComments(selectedTodo)
  }

  async function handleDeleteComment(id: number) {
    if (!selectedTodo) return
    await api.deleteComment(id)
    await loadComments(selectedTodo)
  }

  async function handleDeleteBoard() {
    if (!activeBoard) return
    await api.deleteBoard(activeBoard.id)
    setActiveBoard(null)
    setTodos([])
    setSelectedTodo(null)
    setComments([])
    const b = await refreshBoards()
    if (b.length > 0) selectBoard(b[0])
  }

  return (
    <div className="app">
      <aside className="sidebar">
        <h1>Todo2fast</h1>
        <form onSubmit={handleCreateBoard} className="new-board">
          <input
            value={newBoardName}
            onChange={(e) => setNewBoardName(e.target.value)}
            placeholder="Nouveau tableau…"
          />
          <button type="submit">+</button>
        </form>
        <nav className="board-list">
          {boards.map((b) => (
            <button
              key={b.id}
              className={activeBoard?.id === b.id ? 'board active' : 'board'}
              onClick={() => selectBoard(b)}
            >
              {b.name}
            </button>
          ))}
        </nav>
      </aside>

      <main className="content">
        {error && (
          <div className="error" onClick={() => setError(null)}>
            ⚠️ {error}
          </div>
        )}
        {!activeBoard ? (
          <div className="empty">Créez un tableau pour commencer.</div>
        ) : (
          <>
            <header className="board-header">
              <h2>{activeBoard.name}</h2>
              <button className="danger" onClick={handleDeleteBoard}>
                Supprimer le tableau
              </button>
            </header>

            <div className="todo-pane">
              <form onSubmit={handleCreateTodo} className="new-todo">
                <input
                  value={newTodoTitle}
                  onChange={(e) => setNewTodoTitle(e.target.value)}
                  placeholder="Nouvelle tâche…"
                />
                <button type="submit">Ajouter</button>
              </form>
              <ul className="todo-list">
                {todos.map((t) => (
                  <li
                    key={t.id}
                    className={selectedTodo?.id === t.id ? 'todo active' : 'todo'}
                    onClick={() => handleSelectTodo(t)}
                  >
                    <input
                      type="checkbox"
                      checked={t.done}
                      onChange={() => toggleDone(t)}
                      onClick={(e) => e.stopPropagation()}
                    />
                    <span className={t.done ? 'done' : ''}>{t.title}</span>
                    {t.due_date && <span className="due">📅 {t.due_date}</span>}
                  </li>
                ))}
              </ul>
            </div>

            {selectedTodo && (
              <section className="detail-pane">
                <div className="detail-header">
                  <h3>{selectedTodo.title}</h3>
                  <button className="danger" onClick={() => handleDeleteTodo(selectedTodo.id)}>
                    ✕
                  </button>
                </div>
                {selectedTodo.description && (
                  <p className="description">{selectedTodo.description}</p>
                )}

                <h4>Commentaires</h4>
                <form onSubmit={handleCreateComment} className="new-comment">
                  <input
                    value={author}
                    onChange={(e) => setAuthor(e.target.value)}
                    placeholder="Votre nom"
                    className="author-input"
                  />
                  <input
                    value={newCommentBody}
                    onChange={(e) => setNewCommentBody(e.target.value)}
                    placeholder="Écrire un commentaire…"
                  />
                  <button type="submit">Envoyer</button>
                </form>

                <ul className="comment-list">
                  {comments.map((c) => (
                    <li key={c.id} className="comment">
                      <div className="comment-meta">
                        <strong>{c.author}</strong>
                        <span className="ts">{new Date(c.created_at).toLocaleString()}</span>
                        <button
                          className="danger small"
                          onClick={() => handleDeleteComment(c.id)}
                        >
                          ✕
                        </button>
                      </div>
                      <p>{c.body}</p>
                      <div className="reactions">
                        {EMOJIS.map((emoji) => {
                          const mine = c.reactions.some(
                            (r) => r.author === author && r.emoji === emoji,
                          )
                          const count = c.reactions.filter((r) => r.emoji === emoji).length
                          return (
                            <button
                              key={emoji}
                              className={mine ? 'reaction active' : 'reaction'}
                              onClick={() => handleReaction(c, emoji)}
                            >
                              {emoji} {count > 0 && count}
                            </button>
                          )
                        })}
                      </div>
                    </li>
                  ))}
                </ul>
              </section>
            )}
          </>
        )}
      </main>
    </div>
  )
}

export default App
