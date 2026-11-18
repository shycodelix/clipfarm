"""Deterministic timeline operations.

Every mutation works on the shared project dictionary (seconds, floats) so the
Electron renderer, the MCP server and tests all observe the same result.
"""

from __future__ import annotations

from typing import Any, Dict, Iterable, List, Optional, Sequence, Tuple

from .project import EngineError, find_asset, find_clip, new_id, total_duration

EPSILON = 1e-6
Interval = Tuple[float, float]


def _round(value: float) -> float:
    return round(float(value), 6)


def _clip_end(clip: Dict[str, Any]) -> float:
    return float(clip["start"]) + float(clip["duration"])


def _track_clips(project: Dict[str, Any], track_id: str) -> List[Dict[str, Any]]:
    return [clip for clip in project["clips"] if clip.get("trackId") == track_id]


def place_clip(
    project: Dict[str, Any],
    asset_id: str,
    at: Optional[float] = None,
    duration: Optional[float] = None,
    track_id: str = "v1",
    in_point: float = 0.0,
) -> Dict[str, Any]:
    asset = find_asset(project, asset_id)
    valid_tracks = {track["id"] for track in project["tracks"]}
    if track_id not in valid_tracks:
        raise EngineError(f"Unknown track: {track_id}")
    if track_id == "cc":
        raise EngineError("The captions track is edited through clipfarm_add_caption")

    source_duration = float(asset.get("duration") or 0.0)
    if duration is None:
        duration = max(source_duration - in_point, 0.0) or 5.0
    if duration <= 0:
        raise EngineError("A clip needs a positive duration")
    if in_point < 0:
        raise EngineError("in_point cannot be negative")

    if at is None:
        at = max((_clip_end(clip) for clip in _track_clips(project, track_id)), default=0.0)
    if at < 0:
        raise EngineError("at cannot be negative")

    clip = {
        "id": new_id(),
        "assetId": asset_id,
        "trackId": track_id,
        "start": _round(at),
        "duration": _round(duration),
        "inPoint": _round(in_point),
        "speed": 1.0,
        "volume": 1.0,
        "label": asset.get("name"),
    }
    project["clips"].append(clip)
    return clip


def trim_clip(
    project: Dict[str, Any],
    clip_id: str,
    start: Optional[float] = None,
    duration: Optional[float] = None,
    in_point: Optional[float] = None,
) -> Dict[str, Any]:
    clip = find_clip(project, clip_id)
    if start is not None:
        delta = float(start) - float(clip["start"])
        if start < 0:
            raise EngineError("start cannot be negative")
        clip["inPoint"] = _round(float(clip["inPoint"]) + delta * float(clip.get("speed", 1.0)))
        clip["start"] = _round(start)
        clip["duration"] = _round(max(float(clip["duration"]) - delta, EPSILON))
    if duration is not None:
        if duration <= 0:
            raise EngineError("duration must be positive")
        clip["duration"] = _round(duration)
    if in_point is not None:
        clip["inPoint"] = _round(in_point)
    return clip


def split_clip(project: Dict[str, Any], clip_id: str, at: float) -> Tuple[Dict[str, Any], Dict[str, Any]]:
    clip = find_clip(project, clip_id)
    offset = float(at) - float(clip["start"])
    if offset <= EPSILON or offset >= float(clip["duration"]) - EPSILON:
        raise EngineError("Split point must fall inside the clip")

    left_duration = offset
    right = dict(clip)
    right["id"] = new_id()
    right["start"] = _round(at)
    right["duration"] = _round(float(clip["duration"]) - offset)
    right["inPoint"] = _round(float(clip["inPoint"]) + offset * float(clip.get("speed", 1.0)))
    clip["duration"] = _round(left_duration)
    project["clips"].append(right)
    return clip, right


def move_clip(project: Dict[str, Any], clip_id: str, start: float, track_id: Optional[str] = None) -> Dict[str, Any]:
    clip = find_clip(project, clip_id)
    if start < 0:
        raise EngineError("start cannot be negative")
    if track_id is not None:
        if track_id not in {track["id"] for track in project["tracks"]}:
            raise EngineError(f"Unknown track: {track_id}")
        clip["trackId"] = track_id
    clip["start"] = _round(start)
    return clip


def delete_clip(project: Dict[str, Any], clip_id: str) -> Dict[str, Any]:
    clip = find_clip(project, clip_id)
    project["clips"] = [item for item in project["clips"] if item["id"] != clip_id]
    return clip


def add_caption(
    project: Dict[str, Any],
    start: float,
    end: float,
    text: str,
    words: Optional[Sequence[Dict[str, Any]]] = None,
) -> Dict[str, Any]:
    if end <= start:
        raise EngineError("A caption needs an end time after its start time")
    caption = {
        "id": new_id(),
        "start": _round(start),
        "end": _round(end),
        "text": " ".join(text.split()),
        "words": list(words or []),
    }
    project["captions"].append(caption)
    project["captions"].sort(key=lambda item: item["start"])
    return caption


def captions_from_words(
    words: Sequence[Dict[str, Any]],
    max_chars: int = 64,
    max_duration: float = 5.0,
    gap: float = 0.7,
) -> List[Dict[str, Any]]:
    captions: List[Dict[str, Any]] = []
    bucket: List[Dict[str, Any]] = []

    def flush() -> None:
        if not bucket:
            return
        captions.append(
            {
                "id": new_id(),
                "start": _round(bucket[0]["start"]),
                "end": _round(bucket[-1]["end"]),
                "text": " ".join(word["text"] for word in bucket).strip(),
                "words": [
                    {
                        "text": word["text"],
                        "start": _round(word["start"]),
                        "end": _round(word["end"]),
                        **({"confidence": word["confidence"]} if word.get("confidence") is not None else {}),
                    }
                    for word in bucket
                ],
            }
        )
        bucket.clear()

    for word in words:
        if not word.get("text"):
            continue
        if bucket:
            pending = " ".join(item["text"] for item in bucket + [word])
            previous_end = bucket[-1]["end"]
            if len(pending) > max_chars or float(word["start"]) - float(previous_end) > gap:
                flush()
            elif float(word["end"]) - float(bucket[0]["start"]) > max_duration:
                flush()
        bucket.append(word)
    flush()
    return captions


def _merge(intervals: Iterable[Interval]) -> List[Interval]:
    ordered = sorted((max(0.0, float(a)), max(0.0, float(b))) for a, b in intervals if b > a)
    merged: List[Interval] = []
    for start, end in ordered:
        if merged and start <= merged[-1][1] + EPSILON:
            merged[-1] = (merged[-1][0], max(merged[-1][1], end))
        else:
            merged.append((start, end))
    return merged


def _subtract(interval: Interval, cutters: Sequence[Interval]) -> List[Interval]:
    remaining: List[Interval] = [interval]
    for cut_start, cut_end in cutters:
        next_remaining: List[Interval] = []
        for start, end in remaining:
            if cut_end <= start or cut_start >= end:
                next_remaining.append((start, end))
                continue
            if cut_start > start:
                next_remaining.append((start, cut_start))
            if cut_end < end:
                next_remaining.append((cut_end, end))
        remaining = next_remaining
    return [(start, end) for start, end in remaining if end - start > 0.05]


def remove_silences(
    project: Dict[str, Any],
    silences: Sequence[Dict[str, float]],
    asset_id: Optional[str] = None,
    ripple: bool = True,
) -> Dict[str, Any]:
    """Cut detected silence out of every clip that uses ``asset_id``.

    Silences are expressed in source-media seconds. Clips are shortened, split into
    the audible fragments and, when ``ripple`` is on, everything after them closes
    the gap — the behaviour editors expect from auto-editor style cleanup.
    """
    merged = _merge((item["start"], item["end"]) for item in silences)
    if not merged:
        return {"removed": 0.0, "clips": []}

    targets = [
        clip
        for clip in project["clips"]
        if (asset_id is None or clip.get("assetId") == asset_id) and clip.get("trackId") != "cc"
    ]
    removed_total = 0.0
    created: List[Dict[str, Any]] = []
    timeline_cuts: List[Interval] = []

    for clip in sorted(targets, key=lambda item: float(item["start"])):
        speed = float(clip.get("speed", 1.0)) or 1.0
        start = float(clip["start"])
        end = _clip_end(clip)
        in_point = float(clip["inPoint"])

        # Map source-time silences onto this clip's timeline span.
        cuts: List[Interval] = []
        for silence_start, silence_end in merged:
            cut_start = start + (silence_start - in_point) / speed
            cut_end = start + (silence_end - in_point) / speed
            cut_start = max(start, cut_start)
            cut_end = min(end, cut_end)
            if cut_end - cut_start > 0.05:
                cuts.append((cut_start, cut_end))
        if not cuts:
            continue
        timeline_cuts.extend(cuts)

        removed = sum(cut_end - cut_start for cut_start, cut_end in cuts)
        keep = _subtract((start, end), cuts)
        if not keep:
            project["clips"] = [item for item in project["clips"] if item["id"] != clip["id"]]
            removed_total += removed
            if ripple:
                for other in _track_clips(project, clip["trackId"]):
                    if float(other["start"]) >= end - EPSILON:
                        other["start"] = _round(float(other["start"]) - removed)
            continue

        base = dict(clip)
        project["clips"] = [item for item in project["clips"] if item["id"] != clip["id"]]
        # Re-lay the surviving fragments contiguously so a mid-clip cut closes up.
        cursor = start
        for index, (keep_start, keep_end) in enumerate(keep):
            fragment = dict(base)
            fragment["id"] = clip["id"] if index == 0 else new_id()
            fragment["start"] = _round(cursor)
            fragment["duration"] = _round(keep_end - keep_start)
            fragment["inPoint"] = _round(in_point + (keep_start - start) * speed)
            cursor += keep_end - keep_start
            project["clips"].append(fragment)
            if index > 0:
                created.append(fragment)

        removed_total += removed
        if ripple:
            for other in _track_clips(project, clip["trackId"]):
                if other["id"] == clip["id"]:
                    continue
                if float(other["start"]) >= end - EPSILON:
                    other["start"] = _round(float(other["start"]) - removed)

    project["clips"].sort(key=lambda item: (item["trackId"], float(item["start"])))
    project["silences"] = []
    if ripple and timeline_cuts:
        _remap_captions(project, _merge(timeline_cuts))
    return {"removed": _round(removed_total), "clips": created, "duration": total_duration(project)}


def _remap_captions(project: Dict[str, Any], cuts: Sequence[Interval]) -> None:
    """Compact captions onto the shortened timeline and drop muted speech."""

    def remap(value: float) -> Optional[float]:
        shift = 0.0
        for cut_start, cut_end in cuts:
            if value >= cut_end:
                shift += cut_end - cut_start
            elif value > cut_start:
                return None
            else:
                break
        return value - shift

    kept: List[Dict[str, Any]] = []
    for caption in project.get("captions", []):
        start = remap(float(caption["start"]))
        end = remap(float(caption["end"]))
        if start is None or end is None or end - start <= 0.05:
            continue
        caption["start"] = _round(start)
        caption["end"] = _round(end)
        words = []
        for word in caption.get("words", []):
            word_start = remap(float(word["start"]))
            word_end = remap(float(word["end"]))
            if word_start is None or word_end is None:
                continue
            words.append({**word, "start": _round(word_start), "end": _round(word_end)})
        caption["words"] = words
        kept.append(caption)
    project["captions"] = kept


def apply_scenes(project: Dict[str, Any], scenes: Sequence[Dict[str, Any]], asset_id: str) -> None:
    existing = [
        scene for scene in project.get("scenes", []) if scene.get("assetId") not in (None, asset_id)
    ]
    project["scenes"] = existing + [
        {
            "index": index,
            "assetId": asset_id,
            "start": _round(scene["start"]),
            "end": _round(scene["end"]),
            **({"score": scene["score"]} if scene.get("score") is not None else {}),
        }
        for index, scene in enumerate(scenes)
    ]


def replace_captions(project: Dict[str, Any], captions: Sequence[Dict[str, Any]], asset_id: Optional[str] = None) -> None:
    if asset_id is None:
        project["captions"] = list(captions)
    else:
        source_clip_ids = {
            clip["id"] for clip in project["clips"] if clip.get("assetId") == asset_id
        }
        kept = [
            caption
            for caption in project.get("captions", [])
            if caption.get("clipId") not in source_clip_ids
        ]
        project["captions"] = kept + list(captions)
    project["captions"].sort(key=lambda item: item["start"])
