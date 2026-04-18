-- search_chunks: unité indexable (chunk d'un texte source)
CREATE TABLE IF NOT EXISTS search_chunks (
    id TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL,
    source_type TEXT NOT NULL,
    source_id TEXT,
    chunk_text TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    char_start INTEGER,
    char_end INTEGER,
    created_at TEXT NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_search_chunks_meeting ON search_chunks(meeting_id);
CREATE INDEX IF NOT EXISTS idx_search_chunks_source ON search_chunks(source_type, source_id);

-- search_embeddings: vecteurs denses (une ligne par chunk)
CREATE TABLE IF NOT EXISTS search_embeddings (
    chunk_id TEXT PRIMARY KEY,
    embedding BLOB NOT NULL,
    model_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (chunk_id) REFERENCES search_chunks(id) ON DELETE CASCADE
);

-- FTS5 virtual table backed by search_chunks
CREATE VIRTUAL TABLE IF NOT EXISTS search_chunks_fts USING fts5(
    chunk_text,
    content='search_chunks',
    content_rowid='rowid',
    tokenize='unicode61 remove_diacritics 2'
);

-- Triggers to keep FTS5 in sync
CREATE TRIGGER IF NOT EXISTS search_chunks_ai AFTER INSERT ON search_chunks BEGIN
    INSERT INTO search_chunks_fts(rowid, chunk_text) VALUES (new.rowid, new.chunk_text);
END;
CREATE TRIGGER IF NOT EXISTS search_chunks_ad AFTER DELETE ON search_chunks BEGIN
    INSERT INTO search_chunks_fts(search_chunks_fts, rowid, chunk_text) VALUES('delete', old.rowid, old.chunk_text);
END;
CREATE TRIGGER IF NOT EXISTS search_chunks_au AFTER UPDATE ON search_chunks BEGIN
    INSERT INTO search_chunks_fts(search_chunks_fts, rowid, chunk_text) VALUES('delete', old.rowid, old.chunk_text);
    INSERT INTO search_chunks_fts(rowid, chunk_text) VALUES (new.rowid, new.chunk_text);
END;

-- indexing_state: crash recovery + progress UI
CREATE TABLE IF NOT EXISTS indexing_state (
    meeting_id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    chunks_total INTEGER DEFAULT 0,
    chunks_done INTEGER DEFAULT 0,
    model_id TEXT,
    last_error TEXT,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
