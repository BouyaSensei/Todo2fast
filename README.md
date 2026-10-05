# Todo2fast

Un gestionnaire de tâches collaboratif et rapide, avec **compréhension de documents**.

Déposez un PDF (ou un document). Todo2fast le lit, en extrait les dates et les
actions à faire, et les transforme en tâches — puis vous et votre équipe
collaborez dessus : commentaires, réactions, tableaux partagés.

## Fonctionnalités

- **Tableaux & Tâches** — organisez le travail en tableaux ; créez, modifiez, complétez, supprimez des tâches.
- **Commentaires & Réactions** — discussion sur chaque tâche + réactions emoji (idempotentes par auteur).
- **Collaboration** — invitez des personnes à un tableau via jeton de partage ; rôles (propriétaire / membre). *(à venir)*
- **Upload PDF & extraction de dates** — téléversez un PDF, récupérez son texte et les dates détectées. *(à venir)*
- **Compréhension de documents** — transforme un document en tâches concrètes. Utilise un LLM local (Ollama) ou un modèle cloud quand disponible, avec un repli déterministe pour que ça fonctionne toujours. *(à venir)*

> État actuel : le backend (Rust) et le frontend (React/TypeScript) sont
> fonctionnels et testés — tableaux, tâches, commentaires, réactions.
> La collaboration avancée, l'upload PDF et la compréhension de documents sont en cours.

## Stack

| Couche      | Tech                                        |
|-------------|---------------------------------------------|
| Backend     | Rust — `axum`, SQLite (`rusqlite` bundled)   |
| Frontend    | React + TypeScript (Vite)                 |
| Tests E2E   | Cypress *(à venir)*                          |
| CI          | GitHub Actions : build, clippy, tests, `cargo audit` |

## Développement local

### Backend
```bash
cd backend
cargo run          # sert l'API sur http://localhost:8080
cargo test         # tests unitaires + intégration
cargo fmt --all    # formatage
cargo clippy --all-targets -- -D warnings   # lint strict
```

### Frontend
```bash
cd frontend
npm install
npm run dev        # serveur de dev Vite sur :5173, proxy /api vers le backend :8080
npm run build      # build de production dans dist/
```

## Installation (Windows)

Un binaire release autonome est fourni (SQLite intégré, aucune dépendance système).

**Option 1 — Script PowerShell** (install/désinstall, raccourcis, tâche au démarrage) :
```powershell
# après cargo build --release
.\installer\install.ps1                 # installe dans C:\Program Files\Todo2fast
.\installer\install.ps1 -Port 9000      # port personnalisé
.\installer\install.ps1 -RegisterService   # démarre au boot (admin requis)
.\installer\install.ps1 -Uninstall      # tout retirer
```

**Option 2 — Installateur .exe classique** (wizard Inno Setup, FR/EN) :
```bash
# nécessite Inno Setup 6 installé
"C:\Program Files (x86)\Inno Setup 6\ISCC.exe" installer\todo2fast.iss
# → installer\Output\Todo2fast-Setup-0.1.0.exe
```

**Option 3 — GitHub Release** : pousser un tag déclenche la CI qui compile le
binaire + l'installateur et publie un release téléchargeable :
```bash
git tag v0.1.0 && git push origin v0.1.0
```

## Configuration

| Variable        | Défaut                     | Description                              |
|-----------------|----------------------------|------------------------------------------|
| `T2F_ADDR`      | `0.0.0.0:8080`             | Adresse de bind du serveur API           |
| `T2F_DB_PATH`   | `./todo2fast.sqlite`       | Fichier de base SQLite                   |
| `T2F_LLM_URL`   | `http://localhost:11434`   | Endpoint Ollama pour la compréhension    |
| `T2F_LLM_MODEL` | `llama3.1`                 | Nom du modèle pour la couche LLM         |

## API

| Méthode | Route                                  | Description                    |
|---------|----------------------------------------|--------------------------------|
| GET     | `/api/health`                          | Santé du service               |
| POST    | `/api/boards`                          | Créer un tableau               |
| GET     | `/api/boards`                          | Lister les tableaux            |
| GET     | `/api/boards/:board_id`                | Obtenir un tableau             |
| DELETE  | `/api/boards/:board_id`                | Supprimer un tableau           |
| POST    | `/api/boards/:board_id/todos`          | Créer une tâche                |
| GET     | `/api/boards/:board_id/todos`          | Lister les tâches d'un tableau |
| GET     | `/api/todos/:todo_id`                  | Obtenir une tâche              |
| PUT     | `/api/todos/:todo_id`                  | Mettre à jour une tâche        |
| DELETE  | `/api/todos/:todo_id`                  | Supprimer une tâche            |
| POST    | `/api/todos/:todo_id/comments`         | Créer un commentaire           |
| GET     | `/api/todos/:todo_id/comments`         | Lister les commentaires        |
| GET     | `/api/comments/:comment_id`            | Obtenir un commentaire         |
| DELETE  | `/api/comments/:comment_id`            | Supprimer un commentaire       |
| POST    | `/api/comments/:comment_id/reactions`  | Ajouter une réaction emoji     |
| DELETE  | `/api/comments/:comment_id/reactions?author=&emoji=` | Retirer une réaction |

## Sécurité

La CI impose une porte de sécurité à chaque push : `cargo clippy -D warnings`,
la suite de tests complète, et `cargo audit` (analyse des vulnérabilités des
dépendances). L'API se lie en local par défaut ; activez les jetons d'authentification
avant de l'exposer publiquement.
