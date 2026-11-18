"""Word-level transcription with faster-whisper on quantized int8 weights."""

from __future__ import annotations

import os
import threading
from typing import Any, Callable, Dict, Optional

from .project import EngineError
from .timeline import captions_from_words

INSTALL_HINT = (
    "faster-whisper is not installed. Run `npm run python:setup` "
    "(or `pip install faster-whisper`) to enable transcription."
)

_MODEL_CACHE: Dict[str, Any] = {}


def whisper_available() -> bool:
    try:
        import faster_whisper  # noqa: F401
    except ImportError:
        return False
    return True


def _load_model(model_name: str, compute_type: str, device: str) -> Any:
    key = f"{model_name}:{compute_type}:{device}"
    if key in _MODEL_CACHE:
        return _MODEL_CACHE[key]
    from faster_whisper import WhisperModel

    model_root = os.environ.get("CLIPFARM_MODEL_DIR") or os.path.join(
        os.path.expanduser("~"), ".cache", "clipfarm", "models"
    )
    os.makedirs(model_root, exist_ok=True)
    model = WhisperModel(
        model_name,
        device=device,
        compute_type=compute_type,
        download_root=model_root,
    )
    _MODEL_CACHE[key] = model
    return model


def _preferred_device(requested: str = "auto") -> str:
    if requested and requested != "auto":
        return requested
    try:
        import ctranslate2

        if ctranslate2.get_cuda_device_count() > 0:
            return "cuda"
    except Exception:
        pass
    return "cpu"


def transcribe(
    path: str,
    model: str = "small.en",
    language: str = "auto",
    compute_type: str = "int8",
    vad: bool = True,
    progress: Optional[Callable[[float, str], None]] = None,
    cancel: Optional[threading.Event] = None,
) -> Dict[str, Any]:
    if not whisper_available():
        raise EngineError(INSTALL_HINT)

    device = _preferred_device(os.environ.get("CLIPFARM_WHISPER_DEVICE", "auto"))
    if progress is not None:
        progress(0.02, f"Loading {model} on {device}")

    try:
        whisper = _load_model(model, compute_type, device)
    except Exception as error:
        raise EngineError(f"Could not load the Whisper model `{model}`: {error}") from error

    if progress is not None:
        progress(0.08, "Listening")

    try:
        segments, info = whisper.transcribe(
            path,
            language=None if language in ("auto", "", None) else language,
            word_timestamps=True,
            vad_filter=vad,
            beam_size=5,
            condition_on_previous_text=False,
        )
    except Exception as error:
        raise EngineError(f"Transcription failed: {error}") from error

    duration = float(getattr(info, "duration", 0.0) or 0.0)
    words = []
    for segment in segments:
        if cancel is not None and cancel.is_set():
            raise EngineError("Transcription was cancelled")
        for word in getattr(segment, "words", None) or []:
            text = (word.word or "").strip()
            if not text:
                continue
            words.append(
                {
                    "text": text,
                    "start": float(word.start),
                    "end": float(word.end),
                    "confidence": round(float(getattr(word, "probability", 0.0) or 0.0), 4),
                }
            )
        if progress is not None and duration > 0:
            progress(min(float(segment.end) / duration, 0.98), "Writing word timings")

    captions = captions_from_words(words)
    if progress is not None:
        progress(1.0, f"{len(captions)} captions")

    return {
        "language": getattr(info, "language", language) or "en",
        "languageProbability": round(float(getattr(info, "language_probability", 0.0) or 0.0), 3),
        "duration": duration,
        "captions": captions,
    }
