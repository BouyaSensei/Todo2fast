# Todo2fast

Gestionnaire de tâches collaboratif avec **compréhension de documents** : importez un PDF ou un Markdown, l'application en extrait les tâches (dates, priorités, responsables) et vous propose d'en créer des cartes prêtes à l'emploi.

Backend **Rust/axum** + SQLite · Frontend **React/TypeScript/Vite** · Application de bureau Windows avec **fenêtre native WebView2**.

---

## Fonctionnalités

### Boards & tâches
- **Boards, listes et tâches** : organisation kanban complète (type Trello/Asana).
- **Drag & drop** des tâches entre listes, avec animations.
- **Tags colorés** : créez des tags, colorez-les, attachez-les aux tâches.
- **Commentaires** sur chaque tâche, avec **réactions** (emojis).
- **Membres** : ajoutez des membres à un board, assignez des tâches.

### Compréhension de documents
- **Import PDF et Markdown** (`pdf`, `md`, `markdown`, `txt`) via glisser-déposer.
- **Extraction déterministe** (sans IA) : détection des dates, priorités et responsables dans le texte.
- **Provider IA optionnel** pour affiner l'extraction :
  - **Ollama local** — détecté automatiquement si un serveur tourne sur `localhost:11434`, sans clé API.
  - **OpenAI-compatible** — activé uniquement si une clé est fournie (`T2F_OPENAI_API_KEY`).
- L'IA reste **facultative** : sans provider disponible, l'extraction déterministe fonctionne toujours (résultat marqué `refined: false`).

### Application de bureau Windows
- **Fenêtre native WebView2** (tao + wry) — une application autonome style Asana/Figma, pas un navigateur.
- **Base de données portable** : stockée dans `%LOCALAPPDATA%\Todo2fast` (survit à la réinstallation).
- **Port unique au projet** (`42817`) avec repli automatique sur les ports suivants si celui-ci est occupé.
- **Installateur Inno Setup** — un seul `.exe` à installer.

---

## Démarrage rapide

### Windows (recommandé)

Téléchargez le dernier installateur depuis [GitHub Releases](https://github.com/BouyaSensei/Todo2fast/releases), lancez-le, puis ouvrez **Todo2fast** depuis le menu démarrer. La fenêtre native s'ouvre automatiquement sur l'interface.

### Développement

```bash
# Backend (Rust) — API + frontend en production servés par axum
cd backend
cargo run

# Frontend (React/Vite) — mode dev avec hot-reload, port 5173
cd frontend
npm install
npm run dev
```

Le backend écoute sur `http://localhost:42817` par défaut.

---

## Configuration

Toutes les variables sont **facultatives** ; l'application fonctionne sans aucune d'elles.

| Variable | Description | Défaut |
|----------|-------------|--------|
| `T2F_ADDR` | Adresse d'écoute (host:port) | `0.0.0.0:42817` |
| `T2F_DB_PATH` | Chemin du fichier SQLite | `%LOCALAPPDATA%\Todo2fast\todo2fast.db` (Windows) / `~/.todo2fast/todo2fast.db` |
| `T2F_WEB_DIR` | Dossier du frontend statique à servir | auto (`web/` en install, `../frontend/dist` en dev) |
| `T2F_OLLAMA_BASE_URL` | URL du serveur Ollama (endpoint OpenAI-compatible) | `http://localhost:11434/v1` |
| `T2F_OLLAMA_MODEL` | Modèle Ollama par défaut | `llama3.2` |
| `T2F_OPENAI_API_KEY` | Clé API pour un provider OpenAI-compatible (active ce provider) | — |
| `T2F_OPENAI_BASE_URL` | Base URL d'un provider OpenAI-compatible | — |
| `T2F_AI_MODEL` | Modèle IA à utiliser par défaut | — |

> **Sécurité** : aucune clé API, token ou secret n'est exposé au client ni loggé. Les secrets sont lus côté serveur depuis l'environnement uniquement.

---

## Architecture

```
Todo2fast/
├── backend/          # Rust + axum + SQLite (Tokio)
│   └── src/
│       ├── main.rs   # bootstrap, fenêtre native (Windows), serveur HTTP
│       ├── db.rs     # persistance SQLite
│       ├── api/      # routes REST (boards, todos, documents, ai, …)
│       └── providers/# détection des providers IA (Ollama, OpenAI-compatible)
├── frontend/         # React + TypeScript + Vite
│   └── src/          # composants UI (boards, drag & drop, import, …)
├── installer/        # script Inno Setup + build local
└── .github/workflows/# CI (Rust + Node) et Release (installateur Windows)
```

### API (extrait)
- `GET /api/health` — état du service
- `GET/POST /api/boards`, `GET/PATCH/DELETE /api/boards/:id`
- `…/boards/:id/lists`, `…/boards/:id/todos`, `…/boards/:id/tags`, `…/boards/:id/members`
- `PATCH/DELETE /api/todos/:id`, `…/todos/:id/comments`, `…/todos/:id/tags`
- `POST /api/documents` — upload PDF/Markdown + extraction
- `POST /api/boards/:id/import` — import des tâches extraites dans un board
- `GET /api/ai/providers`, `GET /api/ai/providers/:provider/models`

---

## Qualité & CI

- **Tests** : suite Rust (`cargo test`) couvrant l'API, l'extraction de documents et la détection des providers.
- **Lint** : `cargo clippy` (sans avertissement) + `cargo fmt`.
- **CI GitHub Actions** : vérifie backend (tests/clippy/fmt) et frontend (typecheck/build) à chaque push.
- **Workflow Release** : sur un tag `v*`, compile le backend, le frontend et l'installateur Inno Setup, puis publie la GitHub Release avec le `.exe`.

### Version de l'exécutable = tag GitHub

Le nom de l'installateur suit **toujours** le tag de release. Le workflow passe le tag (sans le `v` initial) à Inno Setup via la variable d'environnement `T2F_VERSION` :

```
tag v0.2.0  →  Todo2fast-Setup-0.2.0.exe
tag v1.0.0  →  Todo2fast-Setup-1.0.0.exe
```

En build local (sans cette variable), le script retombe sur `0.1.0`.

---

## Fonctionnalités à venir

- **Authentification & comptes** : connexion, rôles et permissions par board (partage privé/public).
- **Rappels & échéances** : notifications locales aux dates détectées dans les documents.
- **Plus de formats d'import** : Word (.docx), images avec OCR, export/reprise des boards.
- **Recherche globale** : plein texte sur tâches, commentaires et tags.
- **Multi-utilisateurs temps réel** : synchronisation collaborative (présence, curseurs, conflits).
- **Thèmes & personnalisation** : thèmes clair/sombre, couleurs de board, raccourcis clavier.
- **Sauvegarde & migration** : export JSON/SQLite, restauration, sauvegarde automatique.
- **Plateformes** : build Linux/macOS et packaging (AppImage, .dmg).

---

## Licence

MIT
