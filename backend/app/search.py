"""
Hybrid search module: fuzzy + TF-IDF + semantic (Ollama embeddings).
Degrades gracefully when Ollama is unavailable.
"""

import logging
import math
from collections import defaultdict
from typing import Optional

import numpy as np
from rapidfuzz import fuzz

logger = logging.getLogger(__name__)


class FuzzySearcher:
    """Fuzzy matching with rapidfuzz (Levenshtein-based)."""

    def search(self, query: str, transcripts: list[dict]) -> list[dict]:
        results = []
        query_lower = query.lower()

        for t in transcripts:
            text = t["text"]
            text_lower = text.lower()
            score = fuzz.token_set_ratio(query_lower, text_lower) / 100.0

            if score < 0.4:
                continue

            highlight_start, highlight_end = self._find_best_match_position(text_lower, query_lower)

            results.append({
                "meeting_id": t["meeting_id"],
                "transcript_id": t["transcript_id"],
                "text": text,
                "timestamp": t.get("timestamp", ""),
                "score": score,
                "highlight_start": highlight_start,
                "highlight_end": highlight_end,
                "match_type": "fuzzy",
            })

        return results

    def _find_best_match_position(self, text: str, query: str) -> tuple[int, int]:
        idx = text.find(query)
        if idx >= 0:
            return idx, idx + len(query)

        words = query.split()
        for word in words:
            idx = text.find(word)
            if idx >= 0:
                return idx, idx + len(word)

        return 0, min(len(query), len(text))


class TFIDFSearcher:
    """TF-IDF ranking for keyword-based relevance."""

    def search(self, query: str, transcripts: list[dict], idf_scores: dict[str, float]) -> list[dict]:
        results = []
        query_terms = query.lower().split()

        for t in transcripts:
            text = t["text"]
            text_lower = text.lower()
            words = text_lower.split()
            word_count = len(words) if words else 1

            score = 0.0
            best_start, best_end = 0, 0
            best_term_score = 0.0

            for term in query_terms:
                tf = words.count(term) / word_count
                idf = idf_scores.get(term, 1.0)
                term_score = tf * idf
                score += term_score

                idx = text_lower.find(term)
                if idx >= 0 and term_score > best_term_score:
                    best_start = idx
                    best_end = idx + len(term)
                    best_term_score = term_score

            if score < 0.01:
                continue

            results.append({
                "meeting_id": t["meeting_id"],
                "transcript_id": t["transcript_id"],
                "text": text,
                "timestamp": t.get("timestamp", ""),
                "score": score,
                "highlight_start": best_start,
                "highlight_end": best_end,
                "match_type": "tfidf",
            })

        return results

    @staticmethod
    def compute_idf(transcripts: list[dict]) -> dict[str, float]:
        doc_count = len(transcripts)
        if doc_count == 0:
            return {}

        term_doc_freq: dict[str, int] = defaultdict(int)
        for t in transcripts:
            unique_words = set(t["text"].lower().split())
            for word in unique_words:
                term_doc_freq[word] += 1

        return {
            term: math.log((doc_count + 1) / (freq + 1)) + 1
            for term, freq in term_doc_freq.items()
        }


class SemanticSearcher:
    """Semantic search using Ollama embeddings."""

    def __init__(self, ollama_base_url: str = "http://localhost:11434"):
        self.ollama_base_url = ollama_base_url
        self.model = "nomic-embed-text"

    async def get_embedding(self, text: str) -> Optional[list[float]]:
        import httpx
        try:
            async with httpx.AsyncClient(timeout=30.0) as client:
                response = await client.post(
                    f"{self.ollama_base_url}/api/embed",
                    json={"model": self.model, "input": text},
                )
                if response.status_code == 200:
                    data = response.json()
                    embeddings = data.get("embeddings", [])
                    if embeddings:
                        return embeddings[0]
                return None
        except Exception as e:
            logger.debug(f"Ollama embedding failed: {e}")
            return None

    async def is_available(self) -> bool:
        import httpx
        try:
            async with httpx.AsyncClient(timeout=5.0) as client:
                response = await client.get(f"{self.ollama_base_url}/api/tags")
                if response.status_code == 200:
                    models = response.json().get("models", [])
                    return any(m.get("name", "").startswith(self.model) for m in models)
                return False
        except Exception:
            return False

    def search_embeddings(
        self,
        query_embedding: list[float],
        stored_embeddings: list[dict],
    ) -> list[dict]:
        if not query_embedding or not stored_embeddings:
            return []

        query_vec = np.array(query_embedding)
        query_norm = np.linalg.norm(query_vec)
        if query_norm == 0:
            return []

        results = []
        for entry in stored_embeddings:
            stored_vec = np.frombuffer(entry["embedding"], dtype=np.float32)
            stored_norm = np.linalg.norm(stored_vec)
            if stored_norm == 0:
                continue

            similarity = float(np.dot(query_vec, stored_vec) / (query_norm * stored_norm))

            if similarity < 0.3:
                continue

            results.append({
                "meeting_id": entry["meeting_id"],
                "transcript_id": entry["transcript_id"],
                "text": entry["chunk_text"],
                "timestamp": "",
                "score": similarity,
                "highlight_start": 0,
                "highlight_end": 0,
                "match_type": "semantic",
            })

        return results


class HybridSearchOrchestrator:
    """Orchestrates fuzzy, TF-IDF, and semantic search with weighted score fusion."""

    WEIGHT_FUZZY = 0.2
    WEIGHT_TFIDF = 0.3
    WEIGHT_SEMANTIC = 0.5

    def __init__(self):
        self.fuzzy = FuzzySearcher()
        self.tfidf = TFIDFSearcher()
        self.semantic = SemanticSearcher()

    async def search(
        self,
        query: str,
        transcripts: list[dict],
        stored_embeddings: list[dict],
        limit: int = 20,
    ) -> list[dict]:
        fuzzy_results = self.fuzzy.search(query, transcripts)

        idf_scores = TFIDFSearcher.compute_idf(transcripts)
        tfidf_results = self.tfidf.search(query, transcripts, idf_scores)

        semantic_results = []
        if await self.semantic.is_available():
            query_embedding = await self.semantic.get_embedding(query)
            if query_embedding:
                semantic_results = self.semantic.search_embeddings(query_embedding, stored_embeddings)
        else:
            logger.info("Ollama not available - search without semantic layer")

        merged = self._merge_results(fuzzy_results, tfidf_results, semantic_results)
        meetings = self._group_by_meeting(merged, transcripts, limit)

        return meetings

    def _merge_results(self, fuzzy_results, tfidf_results, semantic_results):
        merged: dict[tuple[str, str], dict] = {}

        has_semantic = len(semantic_results) > 0
        w_fuzzy = self.WEIGHT_FUZZY if has_semantic else 0.4
        w_tfidf = self.WEIGHT_TFIDF if has_semantic else 0.6
        w_semantic = self.WEIGHT_SEMANTIC if has_semantic else 0.0

        for r in fuzzy_results:
            key = (r["meeting_id"], r["transcript_id"])
            if key not in merged:
                merged[key] = {**r, "score": 0.0}
            merged[key]["score"] += r["score"] * w_fuzzy
            if r["highlight_start"] > 0 or merged[key]["highlight_start"] == 0:
                merged[key]["highlight_start"] = r["highlight_start"]
                merged[key]["highlight_end"] = r["highlight_end"]

        for r in tfidf_results:
            key = (r["meeting_id"], r["transcript_id"])
            if key not in merged:
                merged[key] = {**r, "score": 0.0}
            merged[key]["score"] += r["score"] * w_tfidf
            if r["highlight_start"] > 0 and merged[key].get("match_type") != "fuzzy":
                merged[key]["highlight_start"] = r["highlight_start"]
                merged[key]["highlight_end"] = r["highlight_end"]

        for r in semantic_results:
            key = (r["meeting_id"], r["transcript_id"])
            if key not in merged:
                merged[key] = {**r, "score": 0.0}
            merged[key]["score"] += r["score"] * w_semantic
            merged[key]["match_type"] = "semantic" if r["score"] > 0.5 else merged[key].get("match_type", "semantic")

        return merged

    def _group_by_meeting(self, merged, transcripts, limit):
        title_lookup = {}
        for t in transcripts:
            title_lookup[t["meeting_id"]] = t.get("title", "")

        meetings_map: dict[str, dict] = {}
        for (meeting_id, _), match in merged.items():
            if meeting_id not in meetings_map:
                meetings_map[meeting_id] = {
                    "meeting_id": meeting_id,
                    "title": title_lookup.get(meeting_id, ""),
                    "score": 0.0,
                    "matches": [],
                }
            meetings_map[meeting_id]["matches"].append({
                "transcript_id": match["transcript_id"],
                "text": match["text"],
                "timestamp": match["timestamp"],
                "highlight_start": match["highlight_start"],
                "highlight_end": match["highlight_end"],
                "match_type": match["match_type"],
            })
            meetings_map[meeting_id]["score"] = max(
                meetings_map[meeting_id]["score"], match["score"]
            )

        sorted_meetings = sorted(meetings_map.values(), key=lambda m: m["score"], reverse=True)
        return sorted_meetings[:limit]
