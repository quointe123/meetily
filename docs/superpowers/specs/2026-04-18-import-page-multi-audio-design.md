# Design — Page d'import multi-audio

**Date :** 2026-04-18  
**Statut :** Approuvé

---

## Contexte

Actuellement, l'import audio est une modale (`ImportAudioDialog`) ouverte via un contexte global (`ImportDialogContext`), accessible depuis la sidebar et par drag-drop global sur l'app. Elle ne supporte qu'un seul fichier par import.

L'objectif est de remplacer cette modale par une page dédiée `/import`, d'y ajouter le support de **1 à 4 fichiers audio** par réunion avec un ordre défini par l'utilisateur, et de promouvoir la feature au rang de fonctionnalité standard (suppression du flag bêta).

---

## Décisions clés

| Question | Décision |
|---|---|
| Modal → page | Page `/import` dédiée |
| Nombre de fichiers | 1 à 4 maximum |
| Tri des fichiers | Boutons ↑ / ↓ sur chaque carte |
| Marqueur de jonction | Segment texte inséré dans la transcription : `--- Audio 2 — 00:42 ---` |
| Drag-drop global | Supprimé — on passe uniquement par la sidebar |
| Flag bêta | Supprimé — feature standard |
| Approche pipeline | Nouveau command Rust `start_import_multi_command`, traitement séquentiel avec offsets |

---

## Section 1 — Architecture & routing

### Ce qui est supprimé

- `frontend/src/contexts/ImportDialogContext.tsx`
- `frontend/src/components/ImportAudio/ImportAudioDialog.tsx`
- `frontend/src/components/ImportAudio/ImportDropOverlay.tsx`
- Tout le code drag-drop global dans `frontend/src/app/layout.tsx` (listeners `tauri://drag-enter`, `tauri://drag-leave`, `tauri://drag-drop`, états `showDropOverlay`, `showImportDialog`, `importFilePath`, composant `ConditionalImportDialog`)
- Le flag `importAndRetranscribe` dans `betaFeatures` (types + settings UI + toutes les vérifications conditionnelles)

### Ce qui change

- **Sidebar** (`components/Sidebar/index.tsx`) : le bouton `Upload` appelle `router.push('/import')` — plus de `useImportDialog()`, plus de gate bêta, icône toujours visible
- **`layout.tsx`** : retrait de `ImportDialogProvider`, `ImportDropOverlay`, `ConditionalImportDialog` et de tous les états/handlers associés

### Ce qui est ajouté

- `frontend/src/app/import/page.tsx` — page Next.js, s'affiche dans le `MainContent` existant
- `frontend/src/hooks/useMultiImport.ts` — hook autonome (pas de provider global)
- Côté Rust : `start_import_multi_command` dans `frontend/src-tauri/src/audio/import.rs`

---

## Section 2 — Page `/import`

### Layout

```
┌─────────────────────────────────────────┐
│  Importer des fichiers audio            │
│                                         │
│  ┌─────────────────────────────────┐    │
│  │   📁  Glissez vos fichiers ici  │    │
│  │      ou  [Parcourir]            │    │
│  │  MP4, WAV, MP3, FLAC, OGG…      │    │
│  └─────────────────────────────────┘    │
│                                         │
│  ┌──────────────────────────────────┐   │
│  │ 1  🎵 reunion_matin.mp4  42:00  ↓│   │
│  ├──────────────────────────────────┤   │
│  │ 2  🎵 reunion_suite.mp3  18:30  ↑│   │
│  └──────────────────────────────────┘   │
│  Durée totale : 1h 00min                │
│                                         │
│  Titre : [reunion_matin              ]  │
│                                         │
│  ▶ Options avancées                     │
│                                         │
│  [  Annuler  ]  [  Importer  →  ]       │
└─────────────────────────────────────────┘
```

### Comportements

- **Ajout de fichiers :** drag-drop sur la zone OU bouton Parcourir (multi-sélection native autorisée). Chaque fichier est validé immédiatement via `validate_audio_file_command`. Maximum 4 fichiers — au-delà, toast d'erreur.
- **Carte fichier :** affiche numéro d'ordre, nom, durée (`formatDuration`), taille (`formatFileSize`), format. Boutons ↑ / ↓ (↑ désactivé sur le premier, ↓ désactivé sur le dernier). Bouton ✕ pour retirer.
- **Titre :** pré-rempli avec le nom du premier fichier (sans extension), modifiable. Se met à jour si le fichier 1 change, sauf si l'utilisateur l'a déjà modifié manuellement.
- **Durée totale :** Σ des durées de tous les fichiers validés, affichée sous la liste.
- **Options avancées** (accordéon) : sélecteur de langue + sélecteur de modèle, identiques à l'actuelle dialog.
- **Bouton Importer :** désactivé si aucun fichier valide.
- **Pendant l'import :** contrôles gelés, barre de progression globale avec message `"Transcription audio 2 de 3 (18:30)…"`, bouton Annuler actif.
- **Succès :** redirection automatique vers `/meeting-details?id=...`.
- **Erreur :** affichage inline avec bouton "Réessayer".

---

## Section 3 — Pipeline Rust multi-audio

### Nouveaux types

```rust
/// Un fichier audio dans une liste ordonnée pour l'import multi
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioFilePart {
    pub path: String,
    pub order: u32,  // 1-based, déjà trié par le frontend
}
```

### Nouveau command Tauri

```rust
#[tauri::command]
pub async fn start_import_multi_command<R: Runtime>(
    app: AppHandle<R>,
    parts: Vec<AudioFilePart>,  // triés par order croissant
    title: String,
    language: Option<String>,
    model: Option<String>,
    provider: Option<String>,
) -> Result<ImportStarted, String>
```

**Cas 1 fichier :** délègue directement à `run_import` existant (zéro régression).  
**Cas multi :** appelle `run_import_multi` (nouvelle fonction interne).

### Logique `run_import_multi`

Pour chaque fichier dans l'ordre :

1. Vérifier annulation
2. Émettre progression : `"Traitement audio N de M — copie…"`
3. Copier le fichier dans le dossier meeting (nommé `audio_1.ext`, `audio_2.ext`, etc.)
4. Decode + resample → samples 16kHz mono
5. VAD → segments de parole
6. **Si N > 1** : insérer un segment-marqueur dans `all_transcripts` :
   ```
   text = "--- Audio N — HH:MM:SS ---"
   audio_start_time = timestamp_offset_ms / 1000.0
   audio_end_time   = timestamp_offset_ms / 1000.0  // durée 0
   ```
7. Transcrire chaque segment avec offset :
   ```
   segment.start_ms + timestamp_offset_ms
   segment.end_ms   + timestamp_offset_ms
   ```
8. Accumuler dans `all_transcripts`
9. Après le fichier : `timestamp_offset_ms += duration_file_N_ms`

À la fin : `create_meeting_with_transcripts` avec tous les segments accumulés → un seul meeting.

### Progression globale

```
progress% = (offset_cumulé + avancement_fichier_courant) / durée_totale × 100
```

Le message précise : `"Transcription audio N de M (durée_du_fichier_courant)…"`

### Métadonnées

`write_import_metadata` est appelé une fois à la fin avec la durée totale cumulée. Le champ `"source"` vaut `"import_multi"` si plusieurs fichiers, `"import"` si un seul.

---

## Section 4 — Hook `useMultiImport`

### Types

```ts
interface AudioFilePart {
  id: string            // uuid local React (clé de liste)
  info: AudioFileInfo   // path, filename, duration_seconds, size_bytes, format
  validating: boolean
  error: string | null
}

type MultiImportStatus = 'idle' | 'validating' | 'processing' | 'complete' | 'error'
```

### Interface exposée

```ts
{
  files: AudioFilePart[]
  status: MultiImportStatus
  progress: ImportProgress | null
  error: string | null
  isProcessing: boolean

  addFiles: (paths: string[]) => Promise<void>   // valide chaque fichier
  removeFile: (id: string) => void
  moveUp: (id: string) => void
  moveDown: (id: string) => void
  startImport: (title: string, language?: string|null, model?: string|null, provider?: string|null) => Promise<void>
  cancelImport: () => Promise<void>
  reset: () => void
}
```

### Validation

`addFiles` appelle `validate_audio_file_command` pour chaque path. Si le total dépasse 4 fichiers, les fichiers excédentaires sont ignorés avec un toast d'avertissement. Les fichiers avec une erreur de validation sont affichés avec leur message d'erreur (ne bloquent pas les autres).

### Écoute Tauri

Mêmes événements qu'aujourd'hui : `import-progress`, `import-complete`, `import-error`. Pas de nouveaux événements nécessaires.

### Navigation

`startImport` appelle `start_import_multi_command`. Sur `import-complete`, le hook appelle `refetchMeetings()` puis `router.push('/meeting-details?id=...')`.

---

## Fichiers impactés

### Supprimés
- `frontend/src/contexts/ImportDialogContext.tsx`
- `frontend/src/components/ImportAudio/ImportAudioDialog.tsx`
- `frontend/src/components/ImportAudio/ImportDropOverlay.tsx`
- `frontend/src/components/ImportAudio/index.ts`

### Modifiés
- `frontend/src/app/layout.tsx` — retrait import dialog + drag-drop global
- `frontend/src/components/Sidebar/index.tsx` — bouton Upload → `router.push('/import')`, suppression gate bêta
- `frontend/src/types/betaFeatures.ts` — suppression de `importAndRetranscribe`
- `frontend/src/components/BetaSettings.tsx` — `importAndRetranscribe` est la seule feature listée dans `featureOrder` ; le composant est simplifié ou vidé (à conserver vide pour extensions futures)
- `frontend/src/components/MeetingDetails/TranscriptButtonGroup.tsx` — les deux blocs conditionnels `betaFeatures.importAndRetranscribe &&` sont supprimés : le bouton retranscrire est toujours visible
- `frontend/src-tauri/src/audio/import.rs` — ajout `AudioFilePart`, `start_import_multi_command`, `run_import_multi`
- `frontend/src-tauri/src/lib.rs` — enregistrement du nouveau command Tauri

### Créés
- `frontend/src/app/import/page.tsx`
- `frontend/src/hooks/useMultiImport.ts`

---

## Hors scope

- Re-transcription multi-audio depuis la page `meeting-details` (le `RetranscribeDialog` existant n'est pas modifié)
- Drag-drop global sur l'app
- Import de plus de 4 fichiers
- Fusion de meetings existants
