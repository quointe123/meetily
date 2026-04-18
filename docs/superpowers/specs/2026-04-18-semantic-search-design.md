# Semantic Search — Design Spec

**Date** : 2026-04-18
**Auteur** : brainstorming session avec quointe123
**Status** : Design approuvé, en attente review écrite

---

## 1. Contexte

### 1.1 Problème

La page `/meetings` de l'app Meetily propose une barre de recherche sur la liste des meetings passés. L'utilisateur attend un comportement sémantique : trouver la phrase exacte **ainsi que** des variantes proches via des embeddings.

En l'état actuel :

- Le code sémantique **existe** côté backend Python (`backend/app/search.py`) : 3 couches (fuzzy Levenshtein + TF-IDF + sémantique via Ollama `nomic-embed-text`) fusionnées par somme pondérée.
- **Problème 1** : la couche sémantique dépend d'Ollama (modèle `nomic-embed-text`) qui n'est ni installé ni auto-téléchargé par l'app.
- **Problème 2** : le backend FastAPI n'est **pas auto-démarré** par Tauri. L'utilisateur doit lancer `clean_start_backend.sh` à la main. En production, aucun utilisateur ne le fait.
- **Problème 3** : quand le backend est down, le fallback local (`TranscriptsRepository::search_meetings`) est un simple `LOWER(transcript) LIKE ?` — aucune tolérance, aucune sémantique.

Résultat observé : la recherche fait des correspondances exactes uniquement, ce qui correspond au fallback dégradé permanent.

### 1.2 Objectif

Recherche sémantique qui fonctionne **sans action utilisateur**, dès la première ouverture de l'app, sans dépendance au backend Python ni à Ollama.

### 1.3 Non-objectifs

- Remplacer le backend Python pour ses autres responsabilités (storage, génération de résumés, etc.).
- Proposer une recherche multi-utilisateur ou côté serveur.
- Implémenter une base vectorielle type HNSW / IVF (surdimensionné pour le volume cible).

---

## 2. Décisions principales

| Décision | Choix | Alternative rejetée |
|---|---|---|
| Localisation du moteur de recherche | **Rust / Tauri** (option A) | Auto-démarrage backend Python en sidecar (trop lourd, galère Windows) |
| Moteur d'embeddings | **ONNX embarqué via `fastembed-rs`** (option A2) | Ollama HTTP (dépendance externe fragile) |
| Modèle d'embeddings | **`multilingual-e5-small`** (~470 MB, 384 dimensions) | `nomic-embed-text` (nécessite Ollama), `all-MiniLM-L6-v2` (anglais seul) |
| Migration des meetings existants | **Ré-indexation auto en arrière-plan** (option M1) | À la demande ou jamais |
| Portée de la recherche | **Tous les champs textuels** (option P3) : transcripts, titres, summaries, action_items, key_points, notes | Transcripts uniquement |
| Algorithme de fusion | **Reciprocal Rank Fusion (RRF)** | Somme pondérée actuelle (fragile aux échelles de score hétérogènes) |
| Indexation full-text | **SQLite FTS5 virtuel** | `LIKE` actuel (lent, pas de BM25, pas de tokenization propre) |

---

## 3. Architecture

### 3.1 Vue d'ensemble

```
┌─────────────────────────────────────────────────────────────┐
│                    Tauri Frontend (Rust)                     │
│                                                              │
│   ┌──────────────────────────────────────────────────────┐  │
│   │              Hybrid Search Engine                     │  │
│   │  ┌─────────┐  ┌──────────┐  ┌────────────────────┐  │  │
│   │  │ FTS5    │  │  Fuzzy   │  │  Semantic          │  │  │
│   │  │ (exact) │  │ rapidfuzz│  │  fastembed-rs +    │  │  │
│   │  │         │  │ -rs      │  │  multilingual-e5   │  │  │
│   │  └─────────┘  └──────────┘  └────────────────────┘  │  │
│   │                       ↓                              │  │
│   │              Score fusion (RRF)                      │  │
│   └──────────────────────────────────────────────────────┘  │
│                            ↕                                 │
│   ┌──────────────────────────────────────────────────────┐  │
│   │         SQLite locale (existante, enrichie)           │  │
│   │  meetings | transcripts | search_chunks              │  │
│   │  search_embeddings | search_chunks_fts (FTS5)        │  │
│   │  indexing_state                                       │  │
│   └──────────────────────────────────────────────────────┘  │
│                            ↕                                 │
│   Background Indexer (tokio task) — chunke + embedde         │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 Modules Rust (nouveau `frontend/src-tauri/src/search/`)

```
search/
├── mod.rs                    # Exports publics + état global (Embedder singleton)
├── engine.rs                 # HybridSearchEngine — orchestrateur principal
├── embedder.rs               # Wrapper fastembed-rs, cache du modèle en mémoire
├── chunker.rs                # Découpe transcripts + summaries en chunks indexables
├── searchers/
│   ├── mod.rs
│   ├── fts.rs                # SQLite FTS5 (BM25)
│   ├── fuzzy.rs              # rapidfuzz token set ratio (rescore candidats FTS)
│   └── semantic.rs           # Cosine similarity sur embeddings en RAM
├── fusion.rs                 # Reciprocal Rank Fusion (k=60)
├── indexer.rs                # Tâche background : chunke + embedde + stocke
├── migration.rs              # Ré-indexation des meetings existants (M1)
└── commands.rs               # Tauri commands exposées au frontend
```

### 3.3 Responsabilités

| Module | Rôle | Interface publique |
|---|---|---|
| `engine.rs` | Coordonne les 3 searchers en parallèle, fusionne les résultats | `search(query, limit) -> Vec<SearchHit>` |
| `embedder.rs` | Charge le modèle ONNX une fois (lazy), embed à la demande | `embed(&[text]) -> Vec<Vec<f32>>` |
| `chunker.rs` | Split en chunks ~800 chars, overlap 200, tag de la source | `chunk(text, source) -> Vec<Chunk>` |
| `searchers/fts.rs` | Query FTS5 MATCH + BM25 | `search(query, limit) -> Vec<RankedHit>` |
| `searchers/fuzzy.rs` | rapidfuzz sur snippets candidats du FTS | `search(query, candidates) -> Vec<RankedHit>` |
| `searchers/semantic.rs` | Cosine similarity sur vecteurs stockés en RAM | `search(query_vec, limit) -> Vec<RankedHit>` |
| `fusion.rs` | RRF pour combiner les 3 rankings | `fuse(rankings, k=60) -> Vec<SearchHit>` |
| `indexer.rs` | Hook "nouveau transcript → indexe" + API re-index meeting | tokio task + fonctions async |
| `migration.rs` | Détecte meetings non indexés, batch re-embed | tokio task au startup |

### 3.4 Commandes Tauri

```rust
#[tauri::command]
async fn search_meetings(query: String, limit: Option<u32>) -> Result<Vec<SearchHit>, String>;

#[tauri::command]
async fn get_indexing_status() -> Result<IndexingStatus, String>; // pour la barre de progression UI

#[tauri::command]
async fn reindex_all() -> Result<(), String>; // bouton debug "refaire l'index"

#[tauri::command]
async fn semantic_model_download() -> Result<(), String>;

#[tauri::command]
async fn semantic_model_is_ready() -> Result<bool, String>;
```

### 3.5 Embedder singleton

`Embedder` wrappé dans un `OnceCell<Arc<TextEmbedding>>` — le modèle ONNX (~470 MB) n'est chargé en RAM **qu'à la première requête** ou au lancement de l'indexation, jamais au démarrage de l'app. Aucun coût fixe au boot.

### 3.6 Crates à ajouter

- `fastembed = "4"` (ONNX Runtime + modèles multilingues)
- `rapidfuzz = "0.5"` (Levenshtein / token set ratio)
- `rusqlite` avec feature `bundled` (FTS5 déjà inclus)

---

## 4. Schéma de données

### 4.1 Nouvelle migration

`frontend/src-tauri/migrations/20260418000000_add_semantic_search.sql`

**Table `search_chunks`** — unité indexable (chunk d'un texte source)

```sql
CREATE TABLE IF NOT EXISTS search_chunks (
    id TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL,
    source_type TEXT NOT NULL,              -- 'transcript' | 'title' | 'summary' | 'action_items' | 'key_points' | 'notes'
    source_id TEXT,                         -- id du transcript/note/summary source (NULL pour title)
    chunk_text TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,           -- ordre dans le texte source
    char_start INTEGER,                     -- offsets pour l'UI (highlight précis)
    char_end INTEGER,
    created_at TEXT NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
CREATE INDEX idx_search_chunks_meeting ON search_chunks(meeting_id);
CREATE INDEX idx_search_chunks_source ON search_chunks(source_type, source_id);
```

**Table `search_embeddings`** — vecteurs denses (une ligne par chunk)

```sql
CREATE TABLE IF NOT EXISTS search_embeddings (
    chunk_id TEXT PRIMARY KEY,
    embedding BLOB NOT NULL,                -- Vec<f32> sérialisé en little-endian (384 dims × 4 bytes = 1536 bytes)
    model_id TEXT NOT NULL,                 -- 'multilingual-e5-small@v1' — clé d'invalidation
    created_at TEXT NOT NULL,
    FOREIGN KEY (chunk_id) REFERENCES search_chunks(id) ON DELETE CASCADE
);
```

**Index FTS5 virtuel** — pour l'exact rapide

```sql
CREATE VIRTUAL TABLE search_chunks_fts USING fts5(
    chunk_text,
    content='search_chunks',
    content_rowid='rowid',
    tokenize='unicode61 remove_diacritics 2'   -- gère les accents FR
);

-- Triggers de synchro FTS5 ↔ search_chunks
CREATE TRIGGER search_chunks_ai AFTER INSERT ON search_chunks BEGIN
    INSERT INTO search_chunks_fts(rowid, chunk_text) VALUES (new.rowid, new.chunk_text);
END;
CREATE TRIGGER search_chunks_ad AFTER DELETE ON search_chunks BEGIN
    INSERT INTO search_chunks_fts(search_chunks_fts, rowid, chunk_text) VALUES('delete', old.rowid, old.chunk_text);
END;
CREATE TRIGGER search_chunks_au AFTER UPDATE ON search_chunks BEGIN
    INSERT INTO search_chunks_fts(search_chunks_fts, rowid, chunk_text) VALUES('delete', old.rowid, old.chunk_text);
    INSERT INTO search_chunks_fts(rowid, chunk_text) VALUES (new.rowid, new.chunk_text);
END;
```

**Table `indexing_state`** — reprise sur erreur + status UI

```sql
CREATE TABLE IF NOT EXISTS indexing_state (
    meeting_id TEXT PRIMARY KEY,
    status TEXT NOT NULL,                   -- 'pending' | 'chunked' | 'embedded' | 'failed'
    chunks_total INTEGER DEFAULT 0,
    chunks_done INTEGER DEFAULT 0,
    model_id TEXT,
    last_error TEXT,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
```

### 4.2 Stratégie de chunking

| Source | Chunks | Raison |
|---|---|---|
| `meeting.title` | 1 chunk (titre complet) | Court |
| `transcript.transcript` | N chunks de ~800 chars, overlap 200 | Équilibre contexte/granularité |
| `transcript.summary` / `action_items` / `key_points` | 1 chunk par champ | Cohérence sémantique |
| `meeting_notes` | 1 chunk par note | Les notes sont unitaires |

---

## 5. Algorithme de recherche

### 5.1 Flow d'une requête

```
User tape "décision sur le pricing"
            ↓
  ┌──────────────────────────────────────────────────┐
  │   HybridSearchEngine::search(query, limit=20)    │
  │                                                   │
  │   ┌─────────┬──────────┬─────────┐               │
  │   │  FTS5   │ Semantic │  Fuzzy  │  (parallèle) │
  │   └────┬────┴─────┬────┴────┬────┘               │
  │        ↓          ↓         ↓                    │
  │   Top-50 FTS  Top-50 Sem  Top-50 Fuzzy          │
  │        └──────────┴─────────┘                    │
  │                   ↓                              │
  │        Reciprocal Rank Fusion (k=60)             │
  │                   ↓                              │
  │         Dédup par (meeting_id, source)           │
  │                   ↓                              │
  │           Top-20 SearchHit final                 │
  └──────────────────────────────────────────────────┘
            ↓
  Group by meeting_id → UI render
```

### 5.2 FTS5 (exact + tokens)

```sql
SELECT c.id, c.meeting_id, c.chunk_text, c.source_type,
       bm25(search_chunks_fts) AS bm25_score
FROM search_chunks_fts
JOIN search_chunks c ON c.rowid = search_chunks_fts.rowid
WHERE search_chunks_fts MATCH ?
ORDER BY bm25_score LIMIT 50;
```

- Query préparée : escape FTS5 (guillemets doubles sur chaque token), ajout de `*` pour prefix match sur le dernier token.
- `remove_diacritics 2` → "décision" match "decision".

### 5.3 Semantic (cosinus sur embeddings)

- Embed la query avec prefix `query: ` (recommandation du modèle E5).
- Charge **tous les embeddings en RAM** au démarrage de l'engine (384 f32 × N chunks — pour 10 000 chunks : ~15 MB, négligeable).
- Cosine similarity naïve SIMD-friendly (~5 ms pour 10k chunks).
- Seuil minimum : `0.35` (plus strict que le 0.3 historique du backend Python).
- Retourne top-50.

### 5.4 Fuzzy (Levenshtein token set)

- Ne cherche **pas sur toute la base** (coût N²).
- Rescore les **top-200 candidats issus de FTS5** — complémentaire, pas remplaçant.
- `rapidfuzz::token_set_ratio`, seuil 70/100.
- Objectif : tolérer les fautes de frappe que FTS5 rate.

### 5.5 Reciprocal Rank Fusion

```
score_final(chunk) = Σ 1 / (k + rank_i(chunk))    avec k = 60
                    i
```

**Pourquoi RRF** : ne se soucie pas des échelles hétérogènes entre BM25 (0..∞), Levenshtein (0..100) et cosinus (-1..1). Fusion sur les rangs, pas sur les scores bruts. Robuste, zéro tuning de poids.

### 5.6 Multiplicateurs par source (post-fusion)

| Source | ×Multiplicateur | Rationnel |
|---|---|---|
| `title` | 1.3 | Match dans le titre = très significatif |
| `summary` / `key_points` | 1.15 | Contenu condensé, dense en sens |
| `action_items` | 1.1 | Souvent recherché |
| `transcript` | 1.0 | Baseline |
| `notes` | 1.0 | Baseline |

### 5.7 Format de retour

```rust
pub struct SearchHit {
    pub meeting_id: String,
    pub meeting_title: String,
    pub source_type: SourceType,
    pub chunk_text: String,
    pub char_start: Option<i32>,       // pour highlight + auto-scroll (déjà en place côté UI)
    pub char_end: Option<i32>,
    pub score: f32,                     // score RRF fusionné
    pub match_kinds: Vec<MatchKind>,    // ['fts', 'semantic', 'fuzzy']
}
```

---

## 6. Migration des meetings existants (M1)

### 6.1 Flow au premier lancement après update

1. **Boot app** → tâche tokio spawnée : `migration::backfill_embeddings()`
2. **Requête** : `SELECT id FROM meetings WHERE id NOT IN (SELECT meeting_id FROM indexing_state WHERE status='embedded' AND model_id='multilingual-e5-small@v1')`
3. **Pour chaque meeting**, en batch de **4 en parallèle** (pool borné pour ne pas saturer CPU/RAM) :
   - Chunke tous les champs éligibles selon 4.2
   - Embedde en batch (fastembed-rs supporte nativement le batching)
   - Insère dans `search_chunks` + `search_embeddings` en transaction
   - Met à jour `indexing_state`
4. **Progression** émise via event Tauri `semantic-indexing-progress` → bandeau UI

### 6.2 Reprise sur crash

Tout est **idempotent** via `indexing_state`. Un crash au milieu reprend où il en était au prochain boot.

### 6.3 Invalidation sur changement de modèle

Si `model_id` dans `search_embeddings` ≠ modèle courant (changement futur de version), re-embed automatique déclenché par la même tâche de migration.

---

## 7. Onboarding et UX

### 7.1 Intégration au flow existant

Le flow actuel (`OnboardingFlow.tsx` : Welcome → Setup → DownloadProgress → Permissions) ne change pas en nombre d'étapes. On ajoute un **3ème download card** dans `DownloadProgressStep`.

```
┌─────────────────────────────────────────────────┐
│  Préparation de Meetily                          │
│                                                  │
│  ✓ Modèle de transcription (Parakeet)  670 MB   │
│  ✓ Modèle de résumé (Gemma)            815 MB   │
│  ⟳ Moteur de recherche sémantique      470 MB   │
│     [████████░░░░] 67% · 4.2 MB/s               │
│                                                  │
│  [  Continuer  ]                                │
└─────────────────────────────────────────────────┘
```

- **Non-bloquant** (comme Gemma actuellement).
- Téléchargement géré par `fastembed-rs` (depuis HuggingFace) vers `%APPDATA%\Meetily\models\embeddings\`.
- Si déjà présent → skip instantané.
- Events Tauri : `semantic-model-download-progress`, `semantic-model-download-complete`, `semantic-model-download-error`.
- Statut persisté dans `onboarding-status.json` :
  ```json
  { "parakeet": "completed", "gemma": "completed", "semantic_model": "completed" }
  ```

### 7.2 UX de la recherche

Le composant existant (`frontend/src/app/meetings/page.tsx`) change peu :

**Placeholder mis à jour**
```tsx
<input placeholder="Rechercher dans vos meetings (sémantique)..." />
```

**Bandeau discret si indexation en cours**
```
┌──────────────────────────────────────────────────┐
│ 🔍 Rechercher...                                  │
├──────────────────────────────────────────────────┤
│ ⟳ Indexation sémantique : 12/47 meetings         │
│   Les meetings non indexés répondent quand même  │
│   (recherche exacte + fuzzy)                      │
└──────────────────────────────────────────────────┘
```
Se masque quand `chunks_done === chunks_total` partout.

**Résultat enrichi — badges des matchers qui ont trouvé**
```
📄 Réunion produit — 18 avril 2026
[🔤 exact] [🧠 sémantique]
"...on a décidé du pricing à 99€ pour la V1..."
```
Le tableau `match_kinds` permet d'afficher les badges. Les highlights texte sont déjà gérés par le composant existant.

### 7.3 Comportement dégradé

| État | Recherche |
|---|---|
| Modèle pas encore téléchargé | FTS5 + Fuzzy, badge "sémantique en cours de téléchargement" |
| Modèle OK, indexation en cours | FTS5 + Fuzzy sur tous + Sémantique sur les meetings déjà indexés |
| Tout indexé | Les 3 couches sur tout |
| Erreur modèle (corruption) | FTS5 + Fuzzy + bouton "Réparer le moteur sémantique" dans settings |

### 7.4 Gestion des erreurs

- **Download fail** : retry auto 3× avec backoff, puis bouton manuel dans settings.
- **Disque plein** : message clair + check préalable (`Available bytes >= 500MB`).
- **Chargement modèle fail** : log + fallback FTS5+fuzzy, pas de crash.
- **Embedding fail sur un chunk** : skip ce chunk, `indexing_state.status='failed'` pour le meeting, on continue les autres.

---

## 8. Impact sur le code existant

| Fichier | Action |
|---|---|
| `frontend/src-tauri/src/api/api.rs` (lignes 22-56) | Supprimer l'appel HTTP au backend Python `/search-meetings` |
| `frontend/src-tauri/src/database/repositories/transcript.rs::search_meetings` | Garder en interne mais déréférencer de l'API publique |
| `frontend/src/hooks/useSearchMeetings.ts` | Type `SearchMeetingResult` adapté : `match_type: string` → `match_kinds: string[]` |
| `frontend/src/app/meetings/page.tsx` | Ajout du bandeau d'indexation + badges multiples |
| `frontend/src-tauri/src/lib.rs` | Register des nouvelles commandes Tauri |
| `frontend/src-tauri/src/onboarding.rs` | Ajout `semantic_model` dans `ModelStatus` |
| `frontend/src/components/onboarding/steps/DownloadProgressStep.tsx` | 3ème download card + listeners |
| `frontend/src/contexts/OnboardingContext.tsx` | Invocation `semantic_model_download`, listeners events |
| `frontend/src-tauri/Cargo.toml` | Ajout `fastembed`, `rapidfuzz` |

**Backend Python** : aucune modification requise. Sa recherche reste accessible si besoin futur, mais plus sollicitée par le frontend.

---

## 9. Tests

- **Unit** :
  - `chunker` : respect de l'overlap, boundaries sur les mots complets.
  - `embedder` : shape du vecteur (384 dims), batch preserve l'ordre.
  - `fusion` RRF : cas connus (3 rankings, doublons, chunks uniques).
  - FTS5 query escaping : apostrophes, caractères spéciaux, tokens multilingues.
- **Integration** : end-to-end avec 3 meetings fictifs FR — "décision pricing" doit trouver "on s'est mis d'accord sur les tarifs".
- **Perf** : benchmark sur 1000 chunks, objectif `<100 ms` par requête p95.

---

## 10. Risques et points d'attention

| Risque | Mitigation |
|---|---|
| Taille du modèle (~470 MB) ajoutée au footprint d'install | Non-bloquant ; download en tâche de fond ; message clair à l'utilisateur |
| Première indexation longue sur une grosse base existante | Progress bar UX ; recherche dégradée mais fonctionnelle pendant |
| Embeddings en RAM peu scalables au-delà de ~100k chunks | Volume cible meetings personnels = largement en deçà ; si un jour c'est un problème, migration vers index vectoriel type `hnsw-rs` (hors scope) |
| Modèle ONNX non compatible avec certaines archis CPU (AVX absent) | `fastembed-rs` gère les fallbacks ; tester sur une VM CPU basique |
| Migration `20260418000000` : collision avec autres devs | Date UTC unique, conforme au pattern existant |

---

## 11. Résumé des livrables

1. Nouveau module `search/` en Rust (11 fichiers dont 3 searchers)
2. Migration SQL `20260418000000_add_semantic_search.sql`
3. Ajout au flow d'onboarding (3ème download card)
4. Adaptation des types frontend (`match_kinds: string[]`)
5. Suppression du chemin backend Python pour la recherche
6. Tests unitaires et d'intégration
