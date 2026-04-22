#!/usr/bin/env python3
"""Parakeet helper sidecar for Meetily.

Uses istupakov's `onnx-asr` as the reference TDT decoder implementation,
replacing Meetily's custom Rust decoder which suffers from word-drop bugs
comparable to those documented in sherpa-onnx issue #2605.

Protocol: newline-delimited JSON over stdin/stdout. One request per line,
one response per line. Stderr is free-form diagnostic logging (the Rust
side reads it for surfacing errors).

Commands (request):
    {"cmd": "ping"}
    {"cmd": "load_model", "model_dir": "<abs path>",
                           "model_id": "nemo-parakeet-tdt-0.6b-v3",
                           "quantization": "int8"}
    {"cmd": "transcribe_raw", "path": "<abs path to raw f32 le mono 16kHz>",
                               "sample_count": 12345,
                               "language": "fr"}
    {"cmd": "unload"}
    {"cmd": "shutdown"}

Responses:
    {"status": "ok", ...}
    {"status": "error", "message": "...", "traceback": "..."}
"""

from __future__ import annotations

import json
import sys
import traceback
from pathlib import Path
from typing import Any

# Globals holding the long-lived ASR model + VAD. Kept module-scoped so each
# `transcribe_raw` call reuses the already-loaded ONNX sessions.
_model: Any = None
_model_id: str | None = None


def log(msg: str) -> None:
    """Write a diagnostic line to stderr (Rust side surfaces these)."""
    try:
        sys.stderr.write(f"[parakeet-helper] {msg}\n")
        sys.stderr.flush()
    except OSError:
        # Parent closed the pipe. Nothing we can do; don't spam tracebacks.
        pass


def send(obj: dict) -> None:
    """Emit one JSON response line on stdout."""
    try:
        sys.stdout.write(json.dumps(obj) + "\n")
        sys.stdout.flush()
    except OSError:
        # Parent closed stdout (common during shutdown). Swallow quietly.
        pass


def _ensure_config_json(model_dir: Path, model_id: str) -> None:
    """Write config.json if it's missing.

    onnx-asr reads `features_size` from config.json to know the mel-bin
    dimension. Without it, it defaults to 80 — which is wrong for Parakeet
    v3 (128 mels). Meetily's downloader didn't fetch config.json in older
    builds, so we backfill it here.
    """
    config_path = model_dir / "config.json"
    if config_path.is_file():
        return

    # Match istupakov/parakeet-tdt-0.6b-v*-onnx config.json layout.
    if "v3" in model_id:
        cfg = {"model_type": "nemo-conformer-tdt", "features_size": 128, "subsampling_factor": 8}
    else:
        cfg = {"model_type": "nemo-conformer-tdt", "features_size": 80, "subsampling_factor": 8}

    config_path.write_text(json.dumps(cfg))
    log(f"wrote missing config.json ({cfg})")


def handle_load_model(payload: dict) -> dict:
    global _model, _model_id

    import onnx_asr  # imported lazily so `ping` works without deps

    model_dir = Path(payload["model_dir"])
    model_id = payload.get("model_id", "nemo-parakeet-tdt-0.6b-v3")
    quantization = payload.get("quantization", "int8")

    if not model_dir.is_dir():
        return {"status": "error", "message": f"model_dir not found: {model_dir}"}

    _ensure_config_json(model_dir, model_id)

    log(f"loading model_id={model_id} from {model_dir} (quant={quantization})")

    _model = onnx_asr.load_model(
        model_id,
        path=model_dir,
        quantization=quantization,
    )

    # VAD is what makes long-audio recognition work. onnx-asr auto-downloads
    # Silero VAD weights on first use if the `hub` extra is installed.
    try:
        vad = onnx_asr.load_vad("silero")
        # Precision-first (stricter than Meetily's upstream VAD): Meetily's
        # Silero already passed these chunks. The job here is to DROP
        # the false positives that trigger Parakeet v3's "hallucinate a
        # random language on garbage audio" failure mode observed in the
        # HF discussion #12 regression. threshold=0.55 + min_speech=300ms
        # cuts most of that noise.
        _model = _model.with_vad(
            vad,
            threshold=0.55,
            neg_threshold=0.35,
            min_speech_duration_ms=300,
            max_speech_duration_s=18,
            min_silence_duration_ms=200,
            speech_pad_ms=100,
            batch_size=4,
        )
        log("silero VAD attached")
    except Exception as e:
        log(f"VAD load failed, continuing without VAD: {e}")

    _model_id = model_id
    return {"status": "ok", "loaded": model_id}


def _read_raw_f32(path: Path, sample_count: int):
    import numpy as np

    arr = np.fromfile(str(path), dtype=np.float32)
    if arr.size != sample_count:
        raise ValueError(
            f"sample_count mismatch: expected {sample_count}, got {arr.size}"
        )
    return arr


def handle_transcribe_raw(payload: dict) -> dict:
    if _model is None:
        return {"status": "error", "message": "model not loaded"}

    raw_path = Path(payload["path"])
    sample_count = int(payload["sample_count"])
    language = payload.get("language")  # optional, unused by Parakeet v3 but logged

    if not raw_path.is_file():
        return {"status": "error", "message": f"audio file missing: {raw_path}"}

    samples = _read_raw_f32(raw_path, sample_count)
    duration_s = len(samples) / 16000.0
    log(f"transcribe {duration_s:.1f}s, language_hint={language!r}")

    # Parakeet v3 hallucinates a random language on very short / noisy
    # segments (the "stay quiet" training data bias flips to garbage when
    # the input is too short to decide). Drop anything shorter than 1s
    # outright — there's no meaningful French content in a sub-second
    # chunk and the output is almost certainly a hallucination.
    if duration_s < 1.0:
        log(f"skipping short chunk ({duration_s:.2f}s)")
        return {"status": "ok", "segments": []}

    # recognize() accepts a numpy array at a given sample_rate. With VAD
    # attached, it returns a list of result objects; without VAD, a single
    # string. Normalise to a common shape.
    result = _model.recognize(samples, sample_rate=16000)

    segments: list[dict] = []
    if isinstance(result, str):
        segments.append({"text": result, "start_ms": 0, "end_ms": int(duration_s * 1000)})
    else:
        for seg in result:
            text = getattr(seg, "text", None)
            if text is None:
                text = str(seg)
            start = getattr(seg, "start", None) or 0.0
            end = getattr(seg, "end", None) or duration_s
            segments.append(
                {
                    "text": text,
                    "start_ms": int(float(start) * 1000),
                    "end_ms": int(float(end) * 1000),
                }
            )

    return {"status": "ok", "segments": segments}


def handle_unload() -> dict:
    global _model, _model_id
    _model = None
    _model_id = None
    log("model unloaded")
    return {"status": "ok"}


def dispatch(req: dict) -> dict:
    cmd = req.get("cmd")
    if cmd == "ping":
        return {"status": "ok", "pong": True, "model_id": _model_id}
    if cmd == "load_model":
        return handle_load_model(req)
    if cmd == "transcribe_raw":
        return handle_transcribe_raw(req)
    if cmd == "unload":
        return handle_unload()
    return {"status": "error", "message": f"unknown command: {cmd!r}"}


def main() -> int:
    log("ready")
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError as e:
            send({"status": "error", "message": f"invalid JSON: {e}"})
            continue

        if req.get("cmd") == "shutdown":
            send({"status": "ok"})
            log("shutdown requested")
            return 0

        try:
            resp = dispatch(req)
        except Exception as e:  # noqa: BLE001
            resp = {
                "status": "error",
                "message": str(e),
                "traceback": traceback.format_exc(),
            }
        send(resp)

    return 0


if __name__ == "__main__":
    sys.exit(main())
