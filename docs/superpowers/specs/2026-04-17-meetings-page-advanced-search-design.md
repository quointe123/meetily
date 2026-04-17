# Design Spec — Page Meetings + Recherche Avancée

**Date** : 2026-04-17
**Statut** : Approuvé

---

## 1. Contexte et motivation

Actuellement, la liste des meetings est affichée dans la sidebar dépliable, avec une recherche basique (`LIKE` SQL case-insensitive). Les limites :

- L'espace de la sidebar est trop restreint pour explorer ses meetings
- La recherche ne tolère pas les fautes de frappe, ne comprend pas le sens, et ne classe pas par pertinence
- Quand on clique sur un résultat de recherche, on atterrit sur la page du meeting sans savoir où se trouve le passage trouvé

## 2. Vue d'ensemble des changements

1. **Sidebar simplifiée** — toujours compacte (64px), plus de version dépliée
2. **Nouvelle page `/meetings`** — cartes de meetings + barre de recherche avancée
3. **Recherche hybride 3 couches** — fuzzy + TF-IDF + sémantique (Ollama)
4. **Scroll + surlignage** — navigation vers le passage exact dans la transcription
5. **Nettoyage du SidebarProvider** — retrait de la logique de recherche et d'expansion

## 3. Sidebar simplifiée

### Ce qui change

- La sidebar reste **toujours en mode compacte** (64px de large)
- On **supprime** : le mécanisme d'expansion (256px), le dossier "Meeting Notes" dépliable, la barre de recherche inline, la liste de meetings dans la sidebar
- L'icône **NotebookPen** (Meeting Notes) devient un lien de navigation vers `/meetings`
- Les autres icônes (Home, Settings) restent inchangées

### Ce qu'on supprime

- `isCollapsed`, `toggleCollapse`, `setIsCollapsed`
- `expandedFolders`, `toggleFolder`
- `sidebarItems` (structure hiérarchique folder/children)
- La fonction `renderItem` complète (~150 lignes)
- `findMatchingSnippet`
- Le composant de recherche inline de la sidebar

### Fichiers impactés

- `frontend/src/components/Sidebar/index.tsx` — réécriture en simple colonne d'icônes
- `frontend/src/components/Sidebar/SidebarProvider.tsx` — retrait logique expansion/recherche
- `frontend/src/app/layout.tsx` — plus de gestion de largeur variable

## 4. Page `/meetings`

### Route et fichier

- Nouveau fichier : `frontend/src/app/meetings/page.tsx`
- URL : `/meetings`
- Accessible via clic sur l'icône Meeting Notes dans la sidebar

### Layout

```
┌─────────────────────────────────────────────────┐
│  🔍 Rechercher dans les meetings...             │  ← sticky
├─────────────────────────────────────────────────┤
│                                                 │
│  ┌───────────────┐ ┌───────────────┐ ┌────────┐│
│  │ Titre         │ │ Titre         │ │ Titre  ││
│  │ Date · Durée  │ │ Date · Durée  │ │ Date · ││
│  │               │ │               │ │        ││
│  │ Aperçu du     │ │ Aperçu du     │ │ Aperçu ││
│  │ résumé...     │ │ résumé...     │ │ résu...││
│  └───────────────┘ └───────────────┘ └────────┘│
│                                                 │
│  ┌───────────────┐ ┌───────────────┐ ┌────────┐│
│  │ ...           │ │ ...           │ │ ...    ││
│  └───────────────┘ └───────────────┘ └────────┘│
└─────────────────────────────────────────────────┘
```

### Barre de recherche

- Pleine largeur, sticky au scroll
- Placeholder : "Rechercher dans les meetings..."
- Debounce de 300ms avant déclenchement de la recherche
- En mode recherche, les cartes se filtrent pour ne montrer que les meetings avec des résultats
- Un indicateur de chargement pendant la recherche

### Cartes de meeting

Chaque carte affiche :

- **Titre** du meeting (gras, tronqué si trop long)
- **Date** formatée (ex: "15 avr 2026") + **durée** (ex: "32 min")
- **Aperçu du résumé** : 2-3 lignes du début du résumé, ou "Pas de résumé" si non généré
- **En mode recherche** : le snippet du passage trouvé remplace l'aperçu, avec les mots-clés surlignés en jaune
- **Actions** : menu "..." au hover → Renommer / Supprimer

### Grille responsive

- 3 colonnes sur écran large (>1200px)
- 2 colonnes sur écran moyen (768-1200px)
- 1 colonne sur petit écran (<768px)

### Tri

- Par date décroissante (plus récents en premier), pas de sélecteur de tri

### Navigation

- **Sans recherche active** : clic → `/meeting-details?id=X`
- **Avec recherche active** : clic → `/meeting-details?id=X&search=terme&transcript_id=xyz&highlight_start=18&highlight_end=24`

## 5. Recherche avancée hybride

### Architecture 3 couches

```
Requête utilisateur
         ↓
┌────────────────────────────────────────────┐
│         Search Orchestrator (backend)       │
│                                            │
│  1. Fuzzy (rapidfuzz)                      │
│     → correction typos, Levenshtein        │
│     → score de similarité                  │
│                                            │
│  2. TF-IDF                                 │
│     → pondération fréquence/rareté         │
│     → ranking par pertinence textuelle     │
│                                            │
│  3. Semantic (Ollama embeddings)            │
│     → embedding de la query via Ollama     │
│     → cosine similarity vs index           │
│     → trouve synonymes et concepts liés    │
│                                            │
│  Fusion par score pondéré :                │
│  fuzzy: 0.2 | TF-IDF: 0.3 | semantic: 0.5│
└────────────────────────────────────────────┘
         ↓
  Résultats triés par score combiné
```

### Nouvel endpoint API

**`POST /search-meetings`** (remplace `/search-transcripts`)

Request :
```json
{
  "query": "budget réunion",
  "limit": 20
}
```

Response :
```json
[
  {
    "meeting_id": "uuid",
    "title": "Standup Équipe",
    "score": 0.87,
    "matches": [
      {
        "transcript_id": "uuid",
        "text": "...on a parlé du budget pour le Q2...",
        "timestamp": "00:12:34",
        "highlight_start": 18,
        "highlight_end": 24,
        "match_type": "exact|fuzzy|semantic"
      }
    ]
  }
]
```

Chaque résultat contient la liste des `matches` avec position exacte du passage, ce qui permet le scroll et surlignage côté frontend.

### Indexation des embeddings

- Nouvelle table SQLite : `transcript_embeddings(id, transcript_id, chunk_text, embedding BLOB)`
- Les transcripts sont découpés en chunks de ~200 tokens avec overlap de 50 tokens
- Les embeddings sont calculés via Ollama (`nomic-embed-text`) à la sauvegarde de chaque transcript
- L'indexation se fait en arrière-plan, non bloquante pour l'utilisateur
- Un flag `is_indexed` dans la table `transcripts` pour savoir si l'embedding existe

### Dégradation gracieuse

| Ollama actif | Comportement |
|---|---|
| Oui | Fuzzy + TF-IDF + Sémantique (score complet) |
| Non | Fuzzy + TF-IDF uniquement (toujours fonctionnel) |

Le frontend ne sait pas quelle couche a répondu. Il reçoit juste les résultats triés par score.

### Dépendances

- **Backend Python** : `rapidfuzz`, `numpy`
- **Ollama** : modèle `nomic-embed-text` (géré comme les autres modèles Ollama)

## 6. Scroll vers le passage et surlignage

### Navigation depuis la recherche

URL générée au clic sur une carte en mode recherche :
```
/meeting-details?id=abc&search=budget&transcript_id=xyz&highlight_start=18&highlight_end=24
```

### Séquence au chargement

1. La page détecte les params `search` / `transcript_id` dans l'URL
2. Calcule dans quelle page de transcripts se trouve le segment (paginés par 100)
3. Charge la bonne page de transcripts
4. Scroll automatique (smooth) vers le segment ciblé (`scrollIntoView`)
5. Surligne le passage en jaune avec animation fade-in
6. Affiche le bandeau de navigation recherche

### Bandeau de navigation recherche

```
┌──────────────────────────────────────────────────────┐
│  ← Retour   🔍 "budget"    ◄ 2/5 ►    ✕ Fermer     │
└──────────────────────────────────────────────────────┘
```

- **Sticky** en haut de la page, sous le titre du meeting
- Affiche le terme recherché
- **◄ ►** : navigation entre les occurrences dans ce meeting (compteur "2/5")
- Chaque clic sur ◄ ► scrolle vers l'occurrence et la surligne
- Charge la bonne page de transcripts à la volée si l'occurrence est sur une autre page
- **✕ Fermer** : retire le bandeau et les surlignages, affichage normal
- **← Retour** : retourne à `/meetings` avec la recherche préservée

### Styles de surlignage

- Occurrences trouvées : `bg-yellow-200`
- Occurrence active (celle en focus) : `bg-yellow-400` + léger outline
- Animation smooth scroll entre occurrences

## 7. Nettoyage du SidebarProvider

### Ce qu'on retire

| Élément | Raison |
|---|---|
| `searchResults`, `isSearching`, `searchQuery` | Migrent vers hook local dans `/meetings` |
| `searchTranscripts()` | Idem |
| `isCollapsed`, `toggleCollapse` | Sidebar toujours compacte |
| `expandedFolders`, `toggleFolder` | Plus de dossiers dépliables |
| `sidebarItems` (structure folder/children) | Plus nécessaire |

### Ce qu'on garde

| Élément | Raison |
|---|---|
| `meetings` + `fetchMeetings()` | Liste globale, utilisée par `/meetings` et potentiellement ailleurs |
| `currentMeeting` + `setCurrentMeeting()` | Meeting actif |
| `deleteMeeting()`, `renameMeeting()` | Actions globales |

### Nouveau hook

**`useSearchMeetings`** — encapsule la logique de recherche :
- Appel au nouvel endpoint `POST /search-meetings`
- Gestion du debounce (300ms)
- State : `query`, `results`, `isSearching`
- Utilisé uniquement dans la page `/meetings`

## 8. Résumé des fichiers impactés

### Nouveaux fichiers

| Fichier | Contenu |
|---|---|
| `frontend/src/app/meetings/page.tsx` | Page liste des meetings + recherche |
| `frontend/src/hooks/useSearchMeetings.ts` | Hook de recherche avancée |
| `backend/app/search.py` | Module de recherche hybride (fuzzy + TF-IDF + sémantique) |

### Fichiers modifiés

| Fichier | Changement |
|---|---|
| `frontend/src/components/Sidebar/index.tsx` | Réécriture — simple colonne d'icônes |
| `frontend/src/components/Sidebar/SidebarProvider.tsx` | Retrait logique recherche/expansion |
| `frontend/src/app/layout.tsx` | Simplification largeur sidebar |
| `frontend/src/app/meeting-details/page-content.tsx` | Ajout bandeau recherche + surlignage |
| `frontend/src/app/meeting-details/page.tsx` | Lecture params recherche dans l'URL |
| `backend/app/main.py` | Nouvel endpoint `/search-meetings`, retrait `/search-transcripts` |
| `backend/app/db.py` | Table `transcript_embeddings`, indexation, recherche hybride |
| `backend/requirements.txt` | Ajout `rapidfuzz`, `numpy` |

### Fichiers potentiellement supprimés

- Aucun fichier entier supprimé, mais des suppressions massives de code dans la sidebar

## 9. Contraintes

- **Privacy-first** : tout reste local, les embeddings sont calculés et stockés localement
- **Pas de breaking change** : recording, meeting-details (sans recherche), settings restent intacts
- **Performance** : l'indexation des embeddings est asynchrone et non bloquante
- **Dégradation gracieuse** : la recherche fonctionne sans Ollama (fuzzy + TF-IDF)
