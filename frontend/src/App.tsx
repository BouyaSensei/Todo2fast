import { useCallback, useEffect, useRef, useState, type CSSProperties } from 'react'
import { api, type Board, type Comment, type DocumentAnalysis, type List, type Tag, type Todo } from './api'

// ─── Utilitaires ─────────────────────────────────────────────────────────────

const BOARD_COLORS = ['#35c9dd', '#8b7bff', '#5ad19a', '#e8b45a', '#f06a6a', '#5aa9e8']
function boardColor(id: number) { return BOARD_COLORS[id % BOARD_COLORS.length] }

function initials(name: string) {
  const parts = name.trim().split(/\s+/)
  return (parts.length > 1 ? parts[0][0] + parts[1][0] : name.slice(0, 2)).toUpperCase()
}

function fmtDate(iso: string | null): string {
  if (!iso) return ''
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso
  return d.toLocaleDateString('fr-FR', { day: '2-digit', month: 'short', year: 'numeric' })
}

function isOverdue(todo: Todo): boolean {
  if (!todo.due_date || todo.done) return false
  const due = new Date(todo.due_date)
  return due.getTime() < Date.now() - 86_400_000
}

function relTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime()
  const m = Math.floor(diff / 60_000)
  if (m < 1) return "à l'instant"
  if (m < 60) return `il y a ${m} min`
  const h = Math.floor(m / 60)
  if (h < 24) return `il y a ${h} h`
  return fmtDate(iso)
}

const EMOJIS = ['👍', '❤️', '🎉', '👀', '✅']

// Palette de couleurs pour les colonnes (sobre, accents discrets).
const LIST_COLORS = ['#35c9dd', '#8b7bff', '#5ad19a', '#e8b45a', '#f06a6a', '#5aa9e8', '#d16ba5', '#9aa4b2']
// Palette de couleurs pour les tags (identique, cohérence visuelle).
const TAG_COLORS = LIST_COLORS

// ─── Toasts ──────────────────────────────────────────────────────────────────

interface Toast { id: number; msg: string; kind: 'success' | 'error' }
let toastSeq = 0

export default function App() {
  const [boards, setBoards] = useState<Board[]>([])
  const [activeBoardId, setActiveBoardId] = useState<number | null>(null)
  const [user, setUser] = useState(() => localStorage.getItem('t2f_user') || 'Invité')

  const [lists, setLists] = useState<List[]>([])
  const [todos, setTodos] = useState<Todo[]>([])
  const [tags, setTags] = useState<Tag[]>([])
  const [loadingBoard, setLoadingBoard] = useState(false)

  const [toasts, setToasts] = useState<Toast[]>([])
  const [detailTodoId, setDetailTodoId] = useState<number | null>(null)
  const [showImport, setShowImport] = useState(false)
  const [showNewBoard, setShowNewBoard] = useState(false)

  const notify = useCallback((msg: string, kind: Toast['kind'] = 'success') => {
    const id = ++toastSeq
    setToasts(t => [...t, { id, msg, kind }])
    setTimeout(() => setToasts(t => t.filter(x => x.id !== id)), 3400)
  }, [])

  // ─── Chargement des boards ────────────────────────────────────────────────
  const loadBoards = useCallback(async () => {
    try {
      const b = await api.listBoards()
      setBoards(b)
      setActiveBoardId(prev => (prev && b.some(x => x.id === prev)) ? prev : (b[0]?.id ?? null))
    } catch (e) {
      notify(`Backend injoignable — lancez le serveur Rust. (${(e as Error).message})`, 'error')
    }
  }, [notify])

  useEffect(() => { loadBoards() }, [loadBoards])

  // ─── Chargement du board actif (colonnes + todos) ─────────────────────────
  const loadBoard = useCallback(async (boardId: number) => {
    setLoadingBoard(true)
    try {
      const [ls, ts, tg] = await Promise.all([
        api.listLists(boardId),
        api.listTodos(boardId),
        api.listTags(boardId),
      ])
      setLists(ls.sort((a, b) => a.position - b.position))
      setTodos(ts.sort((a, b) => a.position - b.position))
      setTags(tg)
    } catch (e) {
      notify(`Erreur board: ${(e as Error).message}`, 'error')
    } finally {
      setLoadingBoard(false)
    }
  }, [notify])

  useEffect(() => {
    if (activeBoardId != null) loadBoard(activeBoardId)
    else { setLists([]); setTodos([]); setTags([]) }
  }, [activeBoardId, loadBoard])

  // ─── Actions boards ───────────────────────────────────────────────────────
  const createBoard = async (name: string) => {
    try {
      const b = await api.createBoard(name)
      setShowNewBoard(false)
      setBoards(prev => [...prev, b])
      setActiveBoardId(b.id)
      notify('Board créé')
    } catch (e) { notify((e as Error).message, 'error') }
  }

  const deleteBoard = async (id: number) => {
    if (!confirm('Supprimer ce board et toutes ses tâches ?')) return
    try {
      await api.deleteBoard(id)
      setBoards(prev => prev.filter(b => b.id !== id))
      notify('Board supprimé')
    } catch (e) { notify((e as Error).message, 'error') }
  }

  // ─── Actions colonnes ─────────────────────────────────────────────────────
  const addList = async (title: string) => {
    if (!activeBoardId || !title.trim()) return
    try {
      const l = await api.createList(activeBoardId, title.trim())
      setLists(prev => [...prev, l])
      notify('Colonne ajoutée')
    } catch (e) { notify((e as Error).message, 'error') }
  }

  const renameList = async (id: number, title: string) => {
    if (!title.trim()) return
    try {
      await api.updateList(id, { title: title.trim() })
      setLists(prev => prev.map(l => l.id === id ? { ...l, title: title.trim() } : l))
    } catch (e) { notify((e as Error).message, 'error') }
  }

  const setListColor = async (id: number, color: string) => {
    try {
      await api.updateList(id, { color })
      setLists(prev => prev.map(l => l.id === id ? { ...l, color: color || null } : l))
    } catch (e) { notify((e as Error).message, 'error') }
  }

  const deleteList = async (id: number) => {
    if (!confirm('Supprimer cette colonne ? Ses cartes seront déplacées en "Sans colonne".')) return
    try {
      await api.deleteList(id)
      setLists(prev => prev.filter(l => l.id !== id))
      notify('Colonne supprimée')
    } catch (e) { notify((e as Error).message, 'error') }
  }

  // ─── Actions todos ────────────────────────────────────────────────────────
  const addTodo = async (listId: number | null, title: string) => {
    if (!activeBoardId || !title.trim()) return
    try {
      const t = await api.createTodo(activeBoardId, title.trim(), { list_id: listId })
      setTodos(prev => [...prev, t])
      notify('Tâche créée')
    } catch (e) { notify((e as Error).message, 'error') }
  }

  const patchTodo = async (id: number, patch: Partial<Todo>) => {
    try {
      const updated = await api.updateTodo(id, patch)
      setTodos(prev => prev.map(t => t.id === id ? updated : t))
      return updated
    } catch (e) { notify((e as Error).message, 'error'); return null }
  }

  const deleteTodo = async (id: number) => {
    try {
      await api.deleteTodo(id)
      setTodos(prev => prev.filter(t => t.id !== id))
      setDetailTodoId(null)
      notify('Tâche supprimée')
    } catch (e) { notify((e as Error).message, 'error') }
  }

  // ─── Actions tags ─────────────────────────────────────────────────────────
  const addTag = async (name: string, color: string) => {
    if (!activeBoardId || !name.trim()) return
    try {
      const t = await api.createTag(activeBoardId, name.trim(), color)
      setTags(prev => [...prev, t].sort((a, b) => a.name.localeCompare(b.name)))
      notify('Tag créé')
    } catch (e) { notify((e as Error).message, 'error') }
  }

  const deleteTag = async (id: number) => {
    try {
      await api.deleteTag(id)
      setTags(prev => prev.filter(t => t.id !== id))
      setTodos(prev => prev.map(t => ({ ...t, tags: (t.tags ?? []).filter(x => x.id !== id) })))
      notify('Tag supprimé')
    } catch (e) { notify((e as Error).message, 'error') }
  }

  const toggleTagOnTodo = async (todoId: number, tagId: number) => {
    const todo = todos.find(t => t.id === todoId)
    if (!todo) return
    const has = (todo.tags ?? []).some(x => x.id === tagId)
    try {
      const updated = has
        ? await api.removeTagFromTodo(todoId, tagId)
        : await api.addTagToTodo(todoId, tagId)
      setTodos(prev => prev.map(t => t.id === todoId ? updated : t))
    } catch (e) { notify((e as Error).message, 'error') }
  }

  // Drag & drop de cartes entre colonnes
  const onDropCard = async (cardId: number, targetListId: number | null) => {
    const card = todos.find(t => t.id === cardId)
    if (!card || card.list_id === targetListId) return
    await patchTodo(cardId, { list_id: targetListId })
  }

  // ─── Import PDF ───────────────────────────────────────────────────────────
  const onImportPdf = async (file: File) => {
    if (!activeBoardId) return
    try {
      const res = await api.importPdf(activeBoardId, file)
      setTodos(prev => [...prev, ...res.created])
      notify(`${res.created_count} tâche(s) importée(s) depuis le PDF`)
      setShowImport(false)
    } catch (e) { notify((e as Error).message, 'error') }
  }

  const activeBoard = boards.find(b => b.id === activeBoardId) ?? null
  const detailTodo = todos.find(t => t.id === detailTodoId) ?? null

  return (
    <div className="app-shell">
      <Sidebar
        boards={boards}
        activeId={activeBoardId}
        user={user}
        onSelect={setActiveBoardId}
        onNewBoard={() => setShowNewBoard(true)}
        onDeleteBoard={deleteBoard}
        onUserChange={(u) => { setUser(u); localStorage.setItem('t2f_user', u) }}
      />

      <div className="main">
        <BoardHeader
          board={activeBoard}
          onImport={() => setShowImport(true)}
          onNewBoard={() => setShowNewBoard(true)}
        />

        {loadingBoard ? (
          <div className="empty-state"><span className="spinner" /></div>
        ) : !activeBoard ? (
          <div className="empty-state">
            <div>
              <div className="big">▦</div>
              <p>Aucun board. Créez-en un pour commencer.</p>
              <button className="btn btn-primary" onClick={() => setShowNewBoard(true)}>+ Nouveau board</button>
            </div>
          </div>
        ) : (
          <BoardCanvas
            lists={lists}
            todos={todos}
            tags={tags}
            onAddList={addList}
            onRenameList={renameList}
            onSetListColor={setListColor}
            onDeleteList={deleteList}
            onAddTodo={addTodo}
            onDropCard={onDropCard}
            onOpenTodo={setDetailTodoId}
          />
        )}
      </div>

      {detailTodo && (
        <TodoDetailPanel
          todo={detailTodo}
          lists={lists}
          tags={tags}
          user={user}
          onClose={() => setDetailTodoId(null)}
          onPatch={patchTodo}
          onDelete={() => deleteTodo(detailTodo.id)}
          onToggleTag={toggleTagOnTodo}
          onAddTag={addTag}
          onDeleteTag={deleteTag}
          notify={notify}
        />
      )}

      {showImport && activeBoard && (
        <PdfImportModal boardName={activeBoard.name} user={user} onImport={onImportPdf} onClose={() => setShowImport(false)} notify={notify} />
      )}

      {showNewBoard && (
        <NewBoardModal onCreate={createBoard} onClose={() => setShowNewBoard(false)} />
      )}

      <div className="toast-wrap">
        {toasts.map(t => <div key={t.id} className={`toast ${t.kind}`}>{t.msg}</div>)}
      </div>
    </div>
  )
}

// ─── Sidebar ─────────────────────────────────────────────────────────────────

function Sidebar(props: {
  boards: Board[]; activeId: number | null; user: string
  onSelect: (id: number) => void; onNewBoard: () => void; onDeleteBoard: (id: number) => void
  onUserChange: (u: string) => void
}) {
  const [editing, setEditing] = useState(false)
  return (
    <aside className="sidebar">
      <div className="sidebar-brand"><span className="logo">T</span> Todo2fast</div>

      <div className="sidebar-section">
        <div className="sidebar-label">Boards</div>
        {props.boards.map(b => (
          <div key={b.id} style={{ position: 'relative' }}>
            <button
              className={`board-item ${b.id === props.activeId ? 'active' : ''}`}
              onClick={() => props.onSelect(b.id)}
            >
              <span className="dot" style={{ background: boardColor(b.id) }} />
              <span className="name">{b.name}</span>
            </button>
            {b.id === props.activeId && (
              <button
                className="icon-btn"
                style={{ position: 'absolute', right: 6, top: 5, width: 24, height: 24 }}
                title="Supprimer le board"
                onClick={(e) => { e.stopPropagation(); props.onDeleteBoard(b.id) }}
              >✕</button>
            )}
          </div>
        ))}
        <button className="board-item" style={{ color: 'var(--text-2)' }} onClick={props.onNewBoard}>+ Nouveau board</button>
      </div>

      <div className="sidebar-footer">
        {editing ? (
          <input
            autoFocus
            defaultValue={props.user}
            onBlur={(e) => { props.onUserChange(e.target.value.trim() || 'Invité'); setEditing(false) }}
            onKeyDown={(e) => { if (e.key === 'Enter') (e.target as HTMLInputElement).blur() }}
            style={{ width: '100%' }}
          />
        ) : (
          <div className="user-chip" onClick={() => setEditing(true)} title="Cliquer pour changer de nom">
            <span className="avatar">{initials(props.user)}</span>
            <span className="uname">{props.user}</span>
          </div>
        )}
      </div>
    </aside>
  )
}

// ─── En-tête du board ────────────────────────────────────────────────────────

function BoardHeader({ board, onImport, onNewBoard }: { board: Board | null; onImport: () => void; onNewBoard: () => void }) {
  return (
    <header className="board-header">
      {board ? (
        <>
          <span className="dot" style={{ width: 10, height: 10, borderRadius: '50%', background: boardColor(board.id), display: 'inline-block' }} />
          <h1>{board.name}</h1>
        </>
      ) : (
        <h1>Todo2fast</h1>
      )}
      <div className="spacer" />
      {board && <button className="btn btn-primary" onClick={onImport}>📄 Importer un PDF</button>}
      {!board && <button className="btn btn-primary" onClick={onNewBoard}>+ Nouveau board</button>}
    </header>
  )
}

// ─── Canvas kanban ───────────────────────────────────────────────────────────

function BoardCanvas(props: {
  lists: List[]; todos: Todo[]; tags: Tag[]
  onAddList: (title: string) => void; onRenameList: (id: number, title: string) => void
  onSetListColor: (id: number, color: string) => void
  onDeleteList: (id: number) => void; onAddTodo: (listId: number | null, title: string) => void
  onDropCard: (cardId: number, listId: number | null) => void; onOpenTodo: (id: number) => void
}) {
  const [addingList, setAddingList] = useState(false)
  const [newListTitle, setNewListTitle] = useState('')
  // État global du drag & drop : carte en cours + colonne cible (zone de drop large).
  const [draggingId, setDraggingId] = useState<number | null>(null)
  const [dropTarget, setDropTarget] = useState<string | null>(null)

  return (
    <div className={`board-canvas ${draggingId != null ? 'is-dragging' : ''}`}>
      {props.lists.map((l, i) => (
        <Column key={l.id} list={l} todos={props.todos.filter(t => t.list_id === l.id)}
          style={{ animationDelay: `${Math.min(i, 10) * 60}ms` }}
          isDropTarget={dropTarget === `list-${l.id}`}
          onDragStartCard={setDraggingId} onDragEndCard={() => { setDraggingId(null); setDropTarget(null) }}
          onDragOverCol={(over) => setDropTarget(over ? `list-${l.id}` : null)}
          onRename={(t) => props.onRenameList(l.id, t)}
          onSetColor={(c) => props.onSetListColor(l.id, c)}
          onDelete={() => props.onDeleteList(l.id)}
          onAddTodo={(title) => props.onAddTodo(l.id, title)}
          onDropCard={props.onDropCard} onOpenTodo={props.onOpenTodo} />
      ))}

      <UnassignedZone todos={props.todos.filter(t => t.list_id === null)}
        isDropTarget={dropTarget === 'unassigned'}
        onDragOverCol={(over) => setDropTarget(over ? 'unassigned' : null)}
        onDragStartCard={setDraggingId} onDragEndCard={() => { setDraggingId(null); setDropTarget(null) }}
        onDropCard={props.onDropCard} onOpenTodo={props.onOpenTodo} />

      {addingList ? (
        <div className="column" style={{ width: 290 }}>
          <input
            autoFocus value={newListTitle} onChange={(e) => setNewListTitle(e.target.value)}
            placeholder="Nom de la colonne…"
            onKeyDown={(e) => {
              if (e.key === 'Enter') { props.onAddList(newListTitle); setNewListTitle(''); setAddingList(false) }
              if (e.key === 'Escape') { setAddingList(false); setNewListTitle('') }
            }}
            onBlur={() => { if (newListTitle.trim()) props.onAddList(newListTitle); setAddingList(false); setNewListTitle('') }}
          />
        </div>
      ) : (
        <button className="add-column" onClick={() => setAddingList(true)}>+ Ajouter une colonne</button>
      )}
    </div>
  )
}

// ─── Zone « Sans colonne » (tâches importées / non classées) ────────────────

function UnassignedZone(props: {
  todos: Todo[]; isDropTarget: boolean
  onDragOverCol: (over: boolean) => void
  onDragStartCard: (id: number) => void; onDragEndCard: () => void
  onDropCard: (cardId: number, listId: number | null) => void; onOpenTodo: (id: number) => void
}) {
  const empty = props.todos.length === 0
  // Pendant un drag, la zone reste visible pour servir de cible large.
  if (empty && !props.isDropTarget) return null
  return (
    <div
      className={`column unassigned ${props.isDropTarget ? 'drop-target' : ''}`}
      onDragOver={(e) => { e.preventDefault(); props.onDragOverCol(true) }}
      onDragLeave={() => props.onDragOverCol(false)}
      onDrop={(e) => {
        e.preventDefault(); props.onDragOverCol(false)
        const id = Number(e.dataTransfer.getData('text/todo-id'))
        if (id) props.onDropCard(id, null)
      }}
    >
      <div className="column-head">
        <span className="title">Sans colonne</span>
        <span className="count">{props.todos.length}</span>
      </div>
      <div className="column-body drop-zone">
        {props.todos.map((t, i) => (
          <Card key={t.id} todo={t} index={i} onOpen={() => props.onOpenTodo(t.id)}
            onDragStartCard={props.onDragStartCard} onDragEndCard={props.onDragEndCard} />
        ))}
        {empty && <div className="drop-hint">Déposez ici pour retirer la colonne</div>}
      </div>
    </div>
  )
}

// ─── Colonne kanban ──────────────────────────────────────────────────────────

function Column(props: {
  list: List; todos: Todo[]; style?: CSSProperties; isDropTarget: boolean
  onDragStartCard: (id: number) => void; onDragEndCard: () => void; onDragOverCol: (over: boolean) => void
  onRename: (title: string) => void; onSetColor: (color: string) => void; onDelete: () => void
  onAddTodo: (title: string) => void; onDropCard: (cardId: number, listId: number | null) => void
  onOpenTodo: (id: number) => void
}) {
  const [editingTitle, setEditingTitle] = useState(false)
  const [titleDraft, setTitleDraft] = useState(props.list.title)
  const [addingCard, setAddingCard] = useState(false)
  const [cardDraft, setCardDraft] = useState('')
  const [showColors, setShowColors] = useState(false)

  const accent = props.list.color ?? 'transparent'

  return (
    <div
      className={`column ${props.isDropTarget ? 'drop-target' : ''}`}
      style={{ ...props.style, ['--list-accent' as string]: accent }}
      onDragOver={(e) => { e.preventDefault(); props.onDragOverCol(true) }}
      onDragLeave={() => props.onDragOverCol(false)}
      onDrop={(e) => {
        e.preventDefault(); props.onDragOverCol(false)
        const id = Number(e.dataTransfer.getData('text/todo-id'))
        if (id) props.onDropCard(id, props.list.id)
      }}
    >
      <div className="column-head">
        {editingTitle ? (
          <input
            autoFocus value={titleDraft} onChange={(e) => setTitleDraft(e.target.value)}
            style={{ flex: 1 }}
            onKeyDown={(e) => { if (e.key === 'Enter') { props.onRename(titleDraft); setEditingTitle(false) } if (e.key === 'Escape') setEditingTitle(false) }}
            onBlur={() => { props.onRename(titleDraft); setEditingTitle(false) }}
          />
        ) : (
          <span className="title" onDoubleClick={() => setEditingTitle(true)} title="Double-cliquer pour renommer">{props.list.title}</span>
        )}
        <span className="count">{props.todos.length}</span>
        <button
          className={`icon-btn color-btn ${showColors ? 'open' : ''}`}
          style={{ width: 24, height: 24 }}
          title="Changer la couleur"
          onClick={() => setShowColors(v => !v)}
        >◐</button>
        <button className="icon-btn" style={{ width: 24, height: 24 }} title="Supprimer la colonne" onClick={props.onDelete}>✕</button>
      </div>

      {showColors && (
        <>
          <div className="color-popback" onClick={() => setShowColors(false)} />
          <div className="color-pop">
            {LIST_COLORS.map(c => (
              <button
                key={c}
                className={`swatch ${props.list.color === c ? 'active' : ''}`}
                style={{ background: c }}
                title={c}
                onClick={() => { props.onSetColor(c); setShowColors(false) }}
              />
            ))}
            <button
              className="swatch reset"
              title="Couleur par défaut"
              onClick={() => { props.onSetColor(''); setShowColors(false) }}
            >∅</button>
          </div>
        </>
      )}

      <div className="column-body drop-zone">
        {props.todos.map((t, i) => (
          <Card key={t.id} todo={t} index={i} onOpen={() => props.onOpenTodo(t.id)}
            onDragStartCard={props.onDragStartCard} onDragEndCard={props.onDragEndCard} />
        ))}
        {props.todos.length === 0 && <div className="drop-hint">Déposez une carte ici</div>}
      </div>

      <div className="add-card-row">
        {addingCard ? (
          <input
            autoFocus value={cardDraft} onChange={(e) => setCardDraft(e.target.value)}
            placeholder="Titre de la tâche…"
            onKeyDown={(e) => {
              if (e.key === 'Enter') { props.onAddTodo(cardDraft); setCardDraft(''); setAddingCard(false) }
              if (e.key === 'Escape') { setAddingCard(false); setCardDraft('') }
            }}
            onBlur={() => { if (cardDraft.trim()) props.onAddTodo(cardDraft); setAddingCard(false); setCardDraft('') }}
          />
        ) : (
          <button className="add-card-btn" onClick={() => setAddingCard(true)}>+ Ajouter une tâche</button>
        )}
      </div>
    </div>
  )
}

// ─── Carte ───────────────────────────────────────────────────────────────────

function Card({ todo, onOpen, index = 0, onDragStartCard, onDragEndCard }: {
  todo: Todo; onOpen: () => void; index?: number
  onDragStartCard?: (id: number) => void; onDragEndCard?: () => void
}) {
  const [dragging, setDragging] = useState(false)
  return (
    <div
      className={`card ${todo.done ? 'done' : ''} ${dragging ? 'dragging' : ''}`}
      style={{ animation: 't2f-fade-in 0.3s var(--ease-out) both', animationDelay: `${Math.min(index, 12) * 45}ms` }}
      draggable
      onDragStart={(e) => { e.dataTransfer.setData('text/todo-id', String(todo.id)); setDragging(true); onDragStartCard?.(todo.id) }}
      onDragEnd={() => { setDragging(false); onDragEndCard?.() }}
      onClick={onOpen}
    >
      <div className="card-title">{todo.title}</div>
      {(todo.tags ?? []).length > 0 && (
        <div className="card-tags">
          {todo.tags.map(tg => (
            <span key={tg.id} className="tag-pill" style={{ ['--tag-color' as string]: tg.color }}>{tg.name}</span>
          ))}
        </div>
      )}
      <div className="card-meta">
        {todo.due_date && (
          <span className={`badge due ${isOverdue(todo) ? 'overdue' : ''}`}>📅 {fmtDate(todo.due_date)}</span>
        )}
        {todo.description && <span className="badge comments">¶</span>}
      </div>
    </div>
  )
}

// ─── Panneau détail d'une tâche ──────────────────────────────────────────────

function TodoDetailPanel(props: {
  todo: Todo; lists: List[]; tags: Tag[]; user: string
  onClose: () => void; onPatch: (id: number, p: Partial<Todo>) => Promise<Todo | null>
  onDelete: () => void
  onToggleTag: (todoId: number, tagId: number) => void
  onAddTag: (name: string, color: string) => void
  onDeleteTag: (id: number) => void
  notify: (m: string, k?: 'success' | 'error') => void
}) {
  const [title, setTitle] = useState(props.todo.title)
  const [desc, setDesc] = useState(props.todo.description)
  const [due, setDue] = useState(props.todo.due_date ? props.todo.due_date.slice(0, 10) : '')
  const [listId, setListId] = useState<string>(props.todo.list_id ? String(props.todo.list_id) : '')
  const [comments, setComments] = useState<Comment[]>([])
  const [commentDraft, setCommentDraft] = useState('')
  const [showEmoji, setShowEmoji] = useState<number | null>(null)
  const [newTagName, setNewTagName] = useState('')
  const [newTagColor, setNewTagColor] = useState('#35c9dd')

  useEffect(() => {
    api.listComments(props.todo.id).then(setComments).catch(() => {})
  }, [props.todo.id])

  const saveField = async (patch: Partial<Todo>) => { await props.onPatch(props.todo.id, patch) }

  const addComment = async () => {
    if (!commentDraft.trim()) return
    try {
      const c = await api.createComment(props.todo.id, props.user, commentDraft.trim())
      setComments(prev => [...prev, c])
      setCommentDraft('')
    } catch (e) { props.notify((e as Error).message, 'error') }
  }

  const react = async (commentId: number, emoji: string) => {
    try {
      const updated = await api.addReaction(commentId, props.user, emoji)
      setComments(prev => prev.map(c => c.id === commentId ? updated : c))
    } catch (e) { props.notify((e as Error).message, 'error') }
  }

  return (
    <div className="overlay detail-overlay" onClick={props.onClose}>
      <div className="detail-panel" onClick={(e) => e.stopPropagation()}>
        <div className="detail-head">
          <input
            className="title-input" value={title} onChange={(e) => setTitle(e.target.value)}
            onBlur={() => { if (title.trim() && title !== props.todo.title) saveField({ title: title.trim() }) }}
            onKeyDown={(e) => { if (e.key === 'Enter') (e.target as HTMLInputElement).blur() }}
          />
          <div className="todo-actions">
            <button
              className={`done-toggle ${props.todo.done ? 'on' : ''}`}
              onClick={() => saveField({ done: !props.todo.done })}
            >{props.todo.done ? '✓ Terminée' : '○ À faire'}</button>
            <select value={listId} onChange={(e) => { setListId(e.target.value); saveField({ list_id: e.target.value ? Number(e.target.value) : null }) }}>
              <option value="">Sans colonne</option>
              {props.lists.map(l => <option key={l.id} value={l.id}>{l.title}</option>)}
            </select>
            <div className="spacer" style={{ flex: 1 }} />
            <button className="icon-btn" title="Fermer" onClick={props.onClose}>✕</button>
          </div>
        </div>

        <div className="detail-body">
          <div>
            <div className="section-label">Description</div>
            <textarea
              value={desc} onChange={(e) => setDesc(e.target.value)} placeholder="Ajouter une description…"
              onBlur={() => { if (desc !== props.todo.description) saveField({ description: desc }) }}
            />
          </div>

          <div className="field-row">
            <div className="field">
              <label>Date d'échéance</label>
              <input type="date" value={due} onChange={(e) => setDue(e.target.value)}
                onBlur={() => saveField({ due_date: due || null })} />
            </div>
          </div>

          <div>
            <div className="section-label">Tags</div>
            <div className="tag-editor">
              {(props.todo.tags ?? []).map(tg => (
                <span key={tg.id} className="tag-pill on" style={{ ['--tag-color' as string]: tg.color }}>
                  {tg.name}
                  <button className="tag-x" title="Retirer du tag" onClick={() => props.onToggleTag(props.todo.id, tg.id)}>✕</button>
                </span>
              ))}
              {(props.todo.tags ?? []).length === 0 && (
                <span className="c-time">Aucun tag sur cette carte.</span>
              )}
            </div>

            {props.tags.length > 0 && (
              <div className="tag-picker">
                {props.tags.map(tg => {
                  const on = (props.todo.tags ?? []).some(x => x.id === tg.id)
                  return (
                    <span key={tg.id} className={`tag-pill pick ${on ? 'on' : ''}`} style={{ ['--tag-color' as string]: tg.color }}>
                      <button onClick={() => props.onToggleTag(props.todo.id, tg.id)}>{tg.name}</button>
                      <button className="tag-x" title="Supprimer ce tag" onClick={(e) => { e.stopPropagation(); props.onDeleteTag(tg.id) }}>✕</button>
                    </span>
                  )
                })}
              </div>
            )}

            <div className="tag-create">
              <input
                value={newTagName} onChange={(e) => setNewTagName(e.target.value)}
                placeholder="Nouveau tag…"
                onKeyDown={(e) => { if (e.key === 'Enter' && newTagName.trim()) { props.onAddTag(newTagName, newTagColor); setNewTagName('') } }}
              />
              <div className="tag-colors">
                {TAG_COLORS.map(c => (
                  <button key={c} className={`swatch ${newTagColor === c ? 'active' : ''}`} style={{ background: c }} title={c} onClick={() => setNewTagColor(c)} />
                ))}
              </div>
              <button className="btn btn-ghost btn-sm" disabled={!newTagName.trim()} onClick={() => { props.onAddTag(newTagName, newTagColor); setNewTagName('') }}>Créer</button>
            </div>
          </div>

          <div>
            <div className="section-label">Commentaires ({comments.length})</div>
            {comments.map(c => (
              <div key={c.id} className="comment">
                <span className="avatar">{initials(c.author)}</span>
                <div className="c-body">
                  <div className="c-head">
                    <span className="c-author">{c.author}</span>
                    <span className="c-time">{relTime(c.created_at)}</span>
                  </div>
                  <div className="c-text">{c.body}</div>
                  {c.reactions.length > 0 && (
                    <div className="reactions">
                      {c.reactions.map(r => (
                        <span key={r.id} className={`reaction-pill ${r.author === props.user ? 'mine' : ''}`}>
                          {r.emoji} {r.author === props.user ? '' : r.author}
                        </span>
                      ))}
                    </div>
                  )}
                  <div className="c-actions">
                    {showEmoji !== c.id && (
                      <button className="react-btn" onClick={() => setShowEmoji(c.id)}>+ Réagir</button>
                    )}
                  </div>
                  {showEmoji === c.id && (
                    <div className="emoji-row">
                      {EMOJIS.map(em => <button key={em} className="emoji-btn" onClick={() => { react(c.id, em); setShowEmoji(null) }}>{em}</button>)}
                    </div>
                  )}
                </div>
              </div>
            ))}
            <div className="comment-form">
              <input
                value={commentDraft} onChange={(e) => setCommentDraft(e.target.value)}
                placeholder={`Commenter en tant que ${props.user}…`}
                onKeyDown={(e) => { if (e.key === 'Enter') addComment() }}
              />
              <button className="btn btn-primary" onClick={addComment}>Envoyer</button>
            </div>
          </div>

          <div style={{ marginTop: 'auto', paddingTop: 12, borderTop: '1px solid var(--border-soft)', display: 'flex', justifyContent: 'space-between' }}>
            <span className="c-time">Créée le {fmtDate(props.todo.created_at)}</span>
            <button className="btn btn-danger btn-sm" onClick={props.onDelete}>Supprimer la tâche</button>
          </div>
        </div>
      </div>
    </div>
  )
}

// ─── Modale d'import PDF ─────────────────────────────────────────────────────

function PdfImportModal(props: {
  boardName: string; user: string
  onImport: (file: File) => void; onClose: () => void
  notify: (m: string, k?: 'success' | 'error') => void
}) {
  const [file, setFile] = useState<File | null>(null)
  const [analysis, setAnalysis] = useState<DocumentAnalysis | null>(null)
  const [busy, setBusy] = useState(false)
  const [drag, setDrag] = useState(false)
  const inputRef = useRef<HTMLInputElement>(null)

  const analyze = async (f: File) => {
    setFile(f); setAnalysis(null); setBusy(true)
    try {
      const a = await api.analyzePdf(f)
      setAnalysis(a)
    } catch (e) { props.notify((e as Error).message, 'error') } finally { setBusy(false) }
  }

  return (
    <div className="overlay" onClick={props.onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h2>📄 Importer un document PDF</h2>
          <button className="icon-btn" onClick={props.onClose}>✕</button>
        </div>
        <div className="modal-body">
          {!file ? (
            <div
              className={`dropzone ${drag ? 'drag' : ''}`}
              onClick={() => inputRef.current?.click()}
              onDragOver={(e) => { e.preventDefault(); setDrag(true) }}
              onDragLeave={() => setDrag(false)}
              onDrop={(e) => { e.preventDefault(); setDrag(false); const f = e.dataTransfer.files[0]; if (f) analyze(f) }}
            >
              <div className="dz-icon">📄</div>
              <p style={{ margin: 0 }}>Déposez un PDF ici ou cliquez pour choisir</p>
              <p style={{ margin: '6px 0 0', fontSize: 12, color: 'var(--text-2)' }}>Le texte est extrait et les tâches suggérées automatiquement (offline).</p>
              <input ref={inputRef} type="file" accept="application/pdf" hidden
                onChange={(e) => { const f = e.target.files?.[0]; if (f) analyze(f) }} />
            </div>
          ) : (
            <>
              <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                <span style={{ fontSize: 22 }}>📄</span>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontWeight: 600, overflow: 'hidden', textOverflow: 'ellipsis' }}>{file.name}</div>
                  <div className="c-time">{(file.size / 1024).toFixed(0)} Ko</div>
                </div>
                <button className="btn btn-ghost btn-sm" onClick={() => { setFile(null); setAnalysis(null) }}>Changer</button>
              </div>

              {busy && <div style={{ display: 'flex', alignItems: 'center', gap: 10, color: 'var(--text-1)' }}><span className="spinner" /> Analyse du document…</div>}

              {analysis && (
                <>
                  <div className="analysis-summary">
                    <div className="stat-chip"><div className="num">{analysis.page_count}</div><div className="lbl">pages</div></div>
                    <div className="stat-chip"><div className="num">{analysis.char_count}</div><div className="lbl">caractères</div></div>
                    <div className="stat-chip"><div className="num">{analysis.suggested_tasks.length}</div><div className="lbl">tâches suggérées</div></div>
                  </div>

                  {analysis.detected_dates.length > 0 && (
                    <div>
                      <div className="section-label">Dates détectées</div>
                      <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
                        {analysis.detected_dates.map((d, i) => <span key={i} className="badge due">📅 {d}</span>)}
                      </div>
                    </div>
                  )}

                  {analysis.preview && (
                    <div>
                      <div className="section-label">Aperçu du texte</div>
                      <div className="preview-box">{analysis.preview}</div>
                    </div>
                  )}

                  {analysis.suggested_tasks.length > 0 && (
                    <div>
                      <div className="section-label">Tâches qui seront créées sur « {props.boardName} »</div>
                      <div className="suggested-list">
                        {analysis.suggested_tasks.map((t, i) => (
                          <div key={i} className="suggested-item">
                            <input type="checkbox" checked readOnly />
                            <span className="st">{t.title}</span>
                            {t.due_date && <span className="badge due">📅 {fmtDate(t.due_date)}</span>}
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </>
              )}
            </>
          )}
        </div>
        <div className="modal-foot">
          <button className="btn" onClick={props.onClose}>Annuler</button>
          {file && analysis && (
            <button className="btn btn-primary" disabled={busy} onClick={() => props.onImport(file)}>
              Importer {analysis.suggested_tasks.length} tâche(s)
            </button>
          )}
        </div>
      </div>
    </div>
  )
}

// ─── Modale nouveau board ────────────────────────────────────────────────────

function NewBoardModal({ onCreate, onClose }: { onCreate: (name: string) => void; onClose: () => void }) {
  const [name, setName] = useState('')
  return (
    <div className="overlay" onClick={onClose}>
      <div className="modal" style={{ maxWidth: 420 }} onClick={(e) => e.stopPropagation()}>
        <div className="modal-head"><h2>Nouveau board</h2><button className="icon-btn" onClick={onClose}>✕</button></div>
        <div className="modal-body">
          <input
            autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="Nom du board…"
            onKeyDown={(e) => { if (e.key === 'Enter' && name.trim()) onCreate(name.trim()) }}
          />
        </div>
        <div className="modal-foot">
          <button className="btn" onClick={onClose}>Annuler</button>
          <button className="btn btn-primary" disabled={!name.trim()} onClick={() => onCreate(name.trim())}>Créer</button>
        </div>
      </div>
    </div>
  )
}
