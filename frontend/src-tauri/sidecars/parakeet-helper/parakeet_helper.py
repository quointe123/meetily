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
    sys.stderr.write(f"[parakeet-helper] {msg}\n")
    sys.stderr.flush()


def send(obj: dict) -> None:
    """Emit one JSON response line on stdout."""
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def handle_load_model(payload: dict) -> dict:
    global _model, _model_id

    import onnx_asr  # imported lazily so `ping` works without deps

    model_dir = Path(payload["model_dir"])
    model_id = payload.get("model_id", "nemo-parakeet-tdt-0.6b-v3")
    quantization = payload.get("quantization", "int8")

    if not model_dir.is_dir():
        return {"status": "error", "message": f"model_dir not found: {model_dir}"}

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
        # Tuned for recall-first on meeting audio: lower threshold captures
        # soft speech; max 18s keeps every chunk safely under the ONNX 20s
        # upper bound; short min_silence avoids stitching distinct utterances.
        _model = _model.with_vad(
            vad,
            threshold=0.40,
            neg_threshold=0.25,
            min_speech_duration_ms=150,
            max_speech_duration_s=18,
            min_silence_duration_ms=100,
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


def handle_ping(_payload: dict) -> dict:
    return {"status": "ok", "pong": True, "model_id": _model_id}


def handle_unload_cmd(_payload: dict) -> dict:
    return handle_unload()


COMMANDS = {
    "ping": handle_ping,
    "load_model": handle_load_model,
    "transcribe_raw": handle_transcribe_raw,
    "unload": handle_unload_cmd,
}


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

        cmd = req.get("cmd")
        if cmd == "shutdown":
            send({"status": "ok"})
            log("shutdown requested")
            return 0

        handler = COMMANDS.get(cmd)
        if handler is None:
            send({"status": "error", "message": f"unknown command: {cmd!r}"})
            continue

        try:
            resp = handler(req)
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
