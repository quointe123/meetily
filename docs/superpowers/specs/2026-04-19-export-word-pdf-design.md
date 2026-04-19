# Export de rapport en Word et PDF — Design

**Date :** 2026-04-19
**Auteur :** quointe123
**Statut :** Validé (en attente de plan d'implémentation)

## Contexte

L'écran de détail d'une réunion affiche deux colonnes : transcription à gauche, rapport IA généré à droite. Le rapport est produit par le LLM au format BlockNote JSON (avec fallback Markdown et legacy JSON), stocké en base SQLite côté backend.

La toolbar du panneau résumé expose actuellement : **Generate Summary**, **AI Model**, **Template**, **Save**, **Copy**. Un export Markdown existe dans le code (`AISummary/index.tsx:595-606`) mais n'est pas exposé dans la toolbar.

**Limitation actuelle :** pas moyen d'exporter le rapport en Word ou PDF pour le partager hors de l'app. Le bouton Copy ne couvre pas le cas où l'utilisateur veut un fichier joint pour un email ou un drive partagé.

## Objectif

Permettre à l'utilisateur d'exporter le rapport généré dans 3 formats depuis la toolbar :
- **Markdown (.md)** — fichier texte (surface l'export existant)
- **Word (.docx)** — document éditable
- **PDF (.pdf)** — document final

## Design

### 1. UI / UX

**Placement :** nouveau bouton **Export** dans `SummaryUpdaterButtonGroup.tsx`, entre `Copy` et `Save`.

**Apparence :**
- Icône `Download` (lucide-react, déjà installé) + label "Export"
- État **disabled** si aucun résumé généré, avec tooltip : "Générer un résumé d'abord"

**Interaction :**
- Click → ouvre un dropdown Radix (pattern déjà utilisé dans le projet)
- 3 entrées, chacune avec icône + label :
  - 📝 Markdown (.md)
  - 📘 Word (.docx)
  - 📕 PDF (.pdf)

**Feedback :**
- Pendant l'export (~1-3s) : spinner sur l'icône du bouton
- Succès : toast `sonner` "Rapport exporté en {format}" + bouton **"Ouvrir le dossier"** qui invoque `shell.open(downloadDir)`
- Erreur : toast d'erreur avec message explicite

**Justification du dropdown vs 3 boutons séparés :** garde la toolbar compacte, regroupe sémantiquement l'action "exporter" avec choix de format (pattern familier à Google Docs, Notion).

### 2. Contenu exporté

Les 3 formats incluent **en-tête métadonnées + corps du rapport** (sans la transcription).

**En-tête métadonnées :**
- Titre de la réunion (nom enregistré)
- Date de la réunion (formatée en français : `19 avril 2026`)
- Durée de la réunion (si disponible : `1h 23min`)
- Modèle IA utilisé (ex: `Généré avec Claude Sonnet 4.6`)

**Corps :** rapport converti vers les primitives natives du format cible.

**Spécificités par format :**

| Format | En-tête | Pied de page |
|---|---|---|
| Markdown (.md) | Métadonnées en YAML frontmatter | — |
| Word (.docx) | Bloc d'en-tête stylé en haut du document | Numéro de page |
| PDF (.pdf) | Bloc d'en-tête sobre en haut de page 1 (pas de page de garde) | Numéro de page |

### 3. Architecture technique

**Approche :** frontend-only avec libs JS matures. Pas de modification Rust.

**Dépendances nouvelles (`frontend/package.json`) :**
- `docx` (~450 KB, MIT) — génération Word (tableaux natifs, styles Heading Word, listes, etc.)
- `pdfmake` (~850 KB, MIT) — génération PDF (tableaux natifs, typographie, pagination)

`docx` et `pdfmake` sont choisis car ils supportent **tableaux natifs**, titres hiérarchiques, listes imbriquées, gras/italique/code. Alternative écartée : `@react-pdf/renderer` n'a pas de composant tableau natif.

**Nouveaux fichiers :**

```
frontend/src/lib/export/
├── index.ts                    # Point d'entrée : exportSummary(format, meeting, summary)
├── types.ts                    # ExportFormat, ExportOptions, ExportResult
├── metadata.ts                 # buildMetadataHeader(meeting) → structure commune
├── blocknote-to-blocks.ts      # Parse BlockNote JSON → AST intermédiaire
├── exporters/
│   ├── markdown.ts             # AST → string markdown (reprend la logique existante)
│   ├── docx.ts                 # AST → document docx via lib `docx`
│   └── pdf.ts                  # AST → pdfmake definition
└── file-saver.ts               # Sauvegarde Tauri (fs + shell openPath)
```

**Nouveaux composants React :**
- `frontend/src/components/MeetingDetails/ExportDropdown.tsx` — dropdown UI
- `frontend/src/hooks/meeting-details/useExportOperations.ts` — orchestration (aligné avec `useCopyOperations.ts`)

**Pipeline de conversion :**

```
BlockNote JSON (source)
        ↓
   [blocknote-to-blocks.ts]
        ↓
   AST intermédiaire (headings, paragraphs, lists, tables, inline marks)
        ↓
   ┌────┴────┬────────┐
   ↓         ↓        ↓
 .md       .docx    .pdf
```

**Rationale AST intermédiaire :** évite de réécrire 3 fois le parsing BlockNote. Ajouter un format futur (ODT, HTML) = un seul nouveau exporter.

**Fallback formats d'entrée :**
- Si `summary_json` (BlockNote) présent → parser BlockNote
- Sinon si `markdown` présent → parser via un parser markdown léger (`remark-parse` ou équivalent, à choisir à l'implémentation — `react-markdown` utilise déjà `remark` transitivement)
- Sinon legacy JSON → parser directement vers AST

**APIs Tauri utilisées (aucune nouvelle commande Rust) :**
- `@tauri-apps/plugin-fs` : `writeFile`
- `@tauri-apps/api/path` : `downloadDir`
- `@tauri-apps/plugin-shell` : `open` (pour "Ouvrir le dossier")

Vérifier que `tauri-plugin-fs` et `tauri-plugin-shell` sont activés dans `tauri.conf.json` (probable, à confirmer à l'implémentation).

### 4. Flux de sauvegarde

**Nom de fichier :** `{MeetingName}_{YYYY-MM-DD}.{ext}` — ex: `Standup_Equipe_2026-04-19.pdf`

**Slugification :**
- Espaces → `_`
- Accents retirés (`é` → `e`, etc.)
- Caractères interdits Windows/macOS (`<>:"/\|?*`) supprimés
- Collision : suffixe auto `_1`, `_2`, etc.
- Si le nom de réunion est vide ou invalide après slugification : fallback `Meeting_{YYYY-MM-DD}`

**Emplacement :** dossier Downloads de l'OS (cross-platform via Tauri `downloadDir()`).

**Mode :** téléchargement automatique (pas de dialog "Enregistrer sous"). Cohérent avec l'UX demandée (option B du brainstorming).

### 5. Cas limites gérés

| Cas | Comportement |
|---|---|
| Aucun résumé généré | Bouton Export disabled + tooltip |
| Résumé en cours de génération | Bouton disabled pendant |
| Résumé en BlockNote JSON uniquement | Conversion directe depuis BlockNote |
| Résumé en markdown legacy | Parse via `remark` → AST |
| Fichier verrouillé (ouvert dans Word) | Toast erreur "Fermez le fichier et réessayez" |
| Permission refusée sur Downloads | Toast erreur + fallback dialog "Choisir un emplacement" (uniquement en récupération d'erreur, pas le flux par défaut) |
| Résumé très long (>50 pages) | Pas de limite, spinner prolongé |

### 6. Hors scope (YAGNI — reporté en V2 si demande)

- Personnalisation avancée (thèmes, logos, CSS custom)
- Export batch (plusieurs réunions en ZIP)
- Aperçu avant export
- Envoi direct par email
- Inclusion de la transcription complète en annexe
- Page de garde PDF dédiée
- Dossier d'export personnalisé en Settings

## Tests

**Tests unitaires (`frontend/__tests__/lib/export/`) :**
- `blocknote-to-blocks` : chaque type de bloc BlockNote → AST correct
- `metadata.ts` : formatage date FR, durée, slugification
- `exporters/*` : snapshot tests sur petits AST représentatifs (titre + paragraphe + liste + tableau)

**Tests d'intégration :**
- Hook `useExportOperations` : mocks Tauri fs/shell, vérifie orchestration
- `ExportDropdown` : rendu, états disabled, click sur chaque option

**Tests manuels (pas automatisables proprement) :**
- Ouvrir le `.docx` généré dans Word → vérifier titres, tableaux, listes rendus nativement
- Ouvrir le `.pdf` → vérifier pagination, numéro de page, texte sélectionnable
- Tester avec un rapport contenant : H1/H2/H3, gras/italique/code, listes imbriquées, tableau 3×3

## Risques et points de vigilance

- **Polices custom :** les defaults de `pdfmake` (Roboto) sont OK ; si on veut une police corporate plus tard, faut les charger explicitement (~30 LOC).
- **Mapping BlockNote :** les blocs exotiques (callouts, embeds) ne sont probablement pas générés par le LLM, mais prévoir un fallback "paragraphe simple" pour tout type non mappé.
- **Taille installeur :** +~1.3 MB pour les deux libs, acceptable pour une app desktop.
- **Plugins Tauri :** vérifier au début de l'implémentation que `tauri-plugin-fs` et `tauri-plugin-shell` sont bien activés dans `tauri.conf.json` et configurés avec les bonnes permissions (scope `$DOWNLOAD/*`).
