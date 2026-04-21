# parakeet-helper — Python sidecar for reference-grade Parakeet decoding

Meetily's built-in Rust Parakeet decoder is a custom implementation of the
TDT (Token-and-Duration Transducer) decoding loop. As documented in
[sherpa-onnx #2605](https://github.com/k2-fsa/sherpa-onnx/issues/2605),
different consumer-side implementations of the same ONNX export give
materially different results — some drop words silently that the
reference implementation ([`onnx-asr`](https://github.com/istupakov/onnx-asr))
recovers correctly.

This sidecar lets Meetily route Parakeet transcription through that
reference implementation. It runs as a long-lived Python subprocess
spawned by the Rust side; communication is newline-delimited JSON over
stdin/stdout.

## Requirements

- Python **3.10+** (tested with 3.12)
- ~500 MB disk for the ONNX Runtime CPU wheel plus numpy
- The Parakeet model files must already be on disk at
  `%APPDATA%\Meetily\models\parakeet\parakeet-tdt-0.6b-v3-int8\`
  (auto-populated by Meetily's model downloader)

## Setup

From this directory:

```bash
pip install -r requirements.txt
```

On first load the helper auto-downloads the ~2 MB Silero VAD weights via
the `onnx-asr[hub]` extra.

## Protocol

Newline-delimited JSON, one request per stdin line, one response per
stdout line. Diagnostic messages go to stderr (free-form text, prefixed
with `[parakeet-helper]`).

### Commands

```json
{"cmd": "ping"}
{"cmd": "load_model", "model_dir": "/abs/path", "model_id": "nemo-parakeet-tdt-0.6b-v3", "quantization": "int8"}
{"cmd": "transcribe_raw", "path": "/abs/path/to/raw.f32", "sample_count": 640000, "language": "fr"}
{"cmd": "unload"}
{"cmd": "shutdown"}
```

Audio is passed as **raw little-endian f32 mono at 16 kHz**, written to a
temp file by the Rust side. This avoids base64-encoding megabytes of audio
through stdin.

### Responses

```json
{"status": "ok", "segments": [{"text": "...", "start_ms": 0, "end_ms": 2300}]}
{"status": "error", "message": "...", "traceback": "..."}
```

## Why Python and not a Rust port

`onnx-asr` is the actively-maintained reference implementation by the
same author as the ONNX export used by Meetily (istupakov). Re-implementing
its TDT decoder in Rust would re-introduce the exact class of bugs this
sidecar is designed to avoid. A Python subprocess is the lowest-risk path
to reference correctness.
