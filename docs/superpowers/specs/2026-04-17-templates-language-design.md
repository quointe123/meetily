# Design Spec: Fix Templates, Add Audit Report, Add Language Support

**Date**: 2026-04-17
**Status**: Draft
**Scope**: Summary generation templates and language propagation

---

## Problem Statement

The summary generation system has three issues:

1. **4 of 6 templates are broken**: Only `daily_standup` and `standard_meeting` are embedded in the binary (`defaults.rs`). The other 4 (`retrospective`, `project_sync`, `psychiatric_session`, `sales_marketing_client_call`) exist as JSON files but are never found at runtime because the file bundling doesn't copy them to the app resources.

2. **No analytical/audit report mode**: The current `standard_meeting` template produces a concise 1-page summary. There is no option for a detailed multi-page analytical report suited to long meetings.

3. **Reports are always in English**: No language parameter is passed to the LLM prompt. The system prompt in `processor.rs` is entirely in English, so the LLM always responds in English regardless of the user's language selection.

---

## Solution

### Axis 1: Embed Missing Templates

Add 4 `include_str!()` entries in `defaults.rs` for the templates that currently only exist as JSON files:

- `retrospective.json`
- `project_sync.json`
- `psychiatric_session.json` (note: filename has typo `psychatric_session.json` — keep as-is to match existing references)
- `sales_marketing_client_call.json`

**Pattern to follow** (already in `defaults.rs`):
```rust
const DAILY_STANDUP: &str = include_str!("../../../templates/daily_standup.json");
const STANDARD_MEETING: &str = include_str!("../../../templates/standard_meeting.json");
// Add:
const RETROSPECTIVE: &str = include_str!("../../../templates/retrospective.json");
const PROJECT_SYNC: &str = include_str!("../../../templates/project_sync.json");
const PSYCHIATRIC_SESSION: &str = include_str!("../../../templates/psychatric_session.json");
const SALES_MARKETING: &str = include_str!("../../../templates/sales_marketing_client_call.json");
```

Update `get_builtin_template(id)` match arms to return the correct `&str` for each ID.

**Files modified**:
- `frontend/src-tauri/src/summary/templates/defaults.rs`

### Axis 2: New Audit Report Template

Create `frontend/src-tauri/templates/audit_report.json` with 8 sections designed for exhaustive analysis of long meetings:

| # | Section | Format | Purpose |
|---|---------|--------|---------|
| 1 | Executive Summary | paragraph | Concise synthesis of key outcomes |
| 2 | Meeting Context | list | Date, participants, objective, duration |
| 3 | Key Decisions & Rationale | table | Each decision with reasoning and stakeholders |
| 4 | Points of Divergence | list | Disagreements, positions, resolution status |
| 5 | Risk Analysis | table | Risks identified (explicit or implicit), severity, mitigation |
| 6 | Recommendations | table | Prioritized actions with owner and deadline if mentioned |
| 7 | Detailed Discussion Log | list | Chronological topics with key quotes from transcript |
| 8 | Appendices | list | Numbers, references, documents mentioned |

Section instructions will guide the LLM to produce exhaustive content (unlike `standard_meeting` which aims for brevity).

Embed in `defaults.rs` via `include_str!()` following the same pattern.

**Files created**:
- `frontend/src-tauri/templates/audit_report.json`

**Files modified**:
- `frontend/src-tauri/src/summary/templates/defaults.rs` — add `include_str!()` + match arm

### Axis 3: Language Support in Summary Generation

Propagate the user's language selection (`ConfigContext.selectedLanguage`) to the LLM system prompt.

**Data flow**:

```
ConfigContext.selectedLanguage (e.g. "fr")
       |
       v
useSummaryGeneration.ts — reads from context, passes as `language` param
       |
       v
api_process_transcript (Tauri command) — receives `language: Option<String>`
       |
       v
SummaryService::process_transcript_background — forwards `language`
       |
       v
processor.rs — injects language instruction into system prompt
```

**Language instruction logic in `processor.rs`**:

| `language` value | Behavior |
|-----------------|----------|
| `"auto"` | No instruction added. LLM follows transcript language naturally. |
| `"auto-translate"` | No instruction added. Transcript is already translated to English by Whisper. |
| `"fr"` | Add: `"You MUST write the entire report in French. All section titles, content, and analysis must be in French."` |
| `"en"` | Add: `"You MUST write the entire report in English."` |
| Any other ISO code | Map code to full language name, add same instruction pattern. |
| `None` / empty | No instruction added (backward compatibility). |

The mapping from ISO 639-1 code to full language name can be a simple `match` in Rust (covering the same list as `LanguageSelection.tsx`).

**Files modified**:
- `frontend/src/hooks/meeting-details/useSummaryGeneration.ts` — add `language` to `api_process_transcript` call
- `frontend/src-tauri/src/summary/commands.rs` — accept `language: Option<String>`, propagate
- `frontend/src-tauri/src/summary/service.rs` — propagate `language` to processor
- `frontend/src-tauri/src/summary/processor.rs` — inject language instruction in system prompt, add ISO-to-name mapping

---

## What Does NOT Change

- `standard_meeting` template content stays identical (1-page concise format)
- `loader.rs` and `types.rs` are not modified
- Template selection UI components (`SummaryGeneratorButtonGroup`, `SummaryPanel`) are not modified — the new audit template appears automatically in the dropdown
- `ConfigContext` and `LanguageSelection` components remain unchanged
- No file bundling fixes (deferred to future work)

---

## File Change Summary

| File | Action |
|------|--------|
| `frontend/src-tauri/templates/audit_report.json` | **Create** |
| `frontend/src-tauri/src/summary/templates/defaults.rs` | Modify — add 5 `include_str!()` + match arms |
| `frontend/src/hooks/meeting-details/useSummaryGeneration.ts` | Modify — pass `language` param |
| `frontend/src-tauri/src/summary/commands.rs` | Modify — accept + propagate `language` |
| `frontend/src-tauri/src/summary/service.rs` | Modify — propagate `language` |
| `frontend/src-tauri/src/summary/processor.rs` | Modify — inject language instruction in prompt |
