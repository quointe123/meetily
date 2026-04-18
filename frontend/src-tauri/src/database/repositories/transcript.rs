use crate::api::{SearchMatch, SearchMeetingResult, TranscriptSearchResult, TranscriptSegment};
use chrono::Utc;
use sqlx::{Connection, Error as SqlxError, SqlitePool};
use std::collections::HashMap;
use tracing::{error, info};
use uuid::Uuid;

pub struct TranscriptsRepository;

impl TranscriptsRepository {
    /// Saves a new meeting and its associated transcript segments.
    /// This function uses a transaction to ensure that either both the meeting
    /// and all its transcripts are saved, or none of them are.
    pub async fn save_transcript(
        pool: &SqlitePool,
        meeting_title: &str,
        transcripts: &[TranscriptSegment],
        folder_path: Option<String>,
    ) -> Result<String, SqlxError> {
        let meeting_id = format!("meeting-{}", Uuid::new_v4());

        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        let now = Utc::now();

        // 1. Create the new meeting
        let result = sqlx::query(
            "INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&meeting_id)
        .bind(meeting_title)
        .bind(now)
        .bind(now)
        .bind(&folder_path)
        .execute(&mut *transaction)
        .await;

        if let Err(e) = result {
            error!("Failed to create meeting '{}': {}", meeting_title, e);
            transaction.rollback().await?;
            return Err(e);
        }

        info!("Successfully created meeting with id: {}", meeting_id);

        // 2. Save each transcript segment with audio timing fields
        for segment in transcripts {
            let transcript_id = format!("transcript-{}", Uuid::new_v4());
            let result = sqlx::query(
                "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration)
                 VALUES (?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&transcript_id)
            .bind(&meeting_id)
            .bind(&segment.text)
            .bind(&segment.timestamp)
            .bind(segment.audio_start_time)
            .bind(segment.audio_end_time)
            .bind(segment.duration)
            .execute(&mut *transaction)
            .await;

            if let Err(e) = result {
                error!(
                    "Failed to save transcript segment for meeting {}: {}",
                    meeting_id, e
                );
                transaction.rollback().await?;
                return Err(e);
            }
        }

        info!(
            "Successfully saved {} transcript segments for meeting {}",
            transcripts.len(),
            meeting_id
        );

        // Commit the transaction
        transaction.commit().await?;

        Ok(meeting_id)
    }

    /// Searches for a query string within the transcripts.
    /// It returns a list of matching transcripts with context.
    pub async fn search_transcripts(
        pool: &SqlitePool,
        query: &str,
    ) -> Result<Vec<TranscriptSearchResult>, SqlxError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let search_query = format!("%{}%", query.to_lowercase());

        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT m.id, m.title, t.transcript, t.timestamp
             FROM meetings m
             JOIN transcripts t ON m.id = t.meeting_id
             WHERE LOWER(t.transcript) LIKE ?",
        )
        .bind(&search_query)
        .fetch_all(pool)
        .await?;

        let results = rows
            .into_iter()
            .map(|(id, title, transcript, timestamp)| {
                let match_context = Self::get_match_context(&transcript, query);
                TranscriptSearchResult {
                    id,
                    title,
                    match_context,
                    timestamp,
                }
            })
            .collect();

        Ok(results)
    }

    /// Searches transcripts and returns results grouped by meeting in SearchMeetingResult format.
    /// This is the local fallback when the Python backend is unavailable.
    pub async fn search_meetings(
        pool: &SqlitePool,
        query: &str,
        limit: u32,
    ) -> Result<Vec<SearchMeetingResult>, SqlxError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let search_query = format!("%{}%", query.to_lowercase());

        let rows = sqlx::query_as::<_, (String, String, String, String, String)>(
            "SELECT m.id, m.title, t.id, t.transcript, t.timestamp
             FROM meetings m
             JOIN transcripts t ON m.id = t.meeting_id
             WHERE LOWER(t.transcript) LIKE ?
             ORDER BY m.created_at DESC",
        )
        .bind(&search_query)
        .fetch_all(pool)
        .await?;

        // Group by meeting
        let mut meetings_map: HashMap<String, SearchMeetingResult> = HashMap::new();
        let query_lower = query.to_lowercase();

        for (meeting_id, title, transcript_id, transcript, timestamp) in rows {
            let transcript_lower = transcript.to_lowercase();
            let (highlight_start, highlight_end) = match transcript_lower.find(&query_lower) {
                Some(idx) => (idx, idx + query.len()),
                None => (0, 0),
            };

            // Build snippet around the match
            let start = highlight_start.saturating_sub(50);
            let end = (highlight_end + 50).min(transcript.len());
            let text = if start > 0 || end < transcript.len() {
                let mut s = String::new();
                if start > 0 { s.push_str("..."); }
                s.push_str(&transcript[start..end]);
                if end < transcript.len() { s.push_str("..."); }
                s
            } else {
                transcript.clone()
            };

            let search_match = SearchMatch {
                transcript_id,
                text,
                timestamp,
                highlight_start,
                highlight_end,
                match_type: "exact".to_string(),
            };

            meetings_map
                .entry(meeting_id.clone())
                .or_insert_with(|| SearchMeetingResult {
                    meeting_id,
                    title,
                    score: 1.0,
                    matches: Vec::new(),
                })
                .matches
                .push(search_match);
        }

        let mut results: Vec<SearchMeetingResult> = meetings_map.into_values().collect();
        results.truncate(limit as usize);
        Ok(results)
    }

    /// Helper function to extract a snippet of text around the first match of a query.
    fn get_match_context(transcript: &str, query: &str) -> String {
        let transcript_lower = transcript.to_lowercase();
        let query_lower = query.to_lowercase();

        match transcript_lower.find(&query_lower) {
            Some(match_index) => {
                let start_index = match_index.saturating_sub(100);
                let end_index = (match_index + query.len() + 100).min(transcript.len());

                let mut context = String::new();
                if start_index > 0 {
                    context.push_str("...");
                }
                context.push_str(&transcript[start_index..end_index]);
                if end_index < transcript.len() {
                    context.push_str("...");
                }
                context
            }
            None => transcript.chars().take(200).collect(), // Fallback to the start of the transcript
        }
    }
}
