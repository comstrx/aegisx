"""The Python side of the versioned Rust/ONNX input contract. No source/label IDs."""
import json
import math
from importlib.resources import files

import numpy as np

from ..content import Content
from ..features import Features


class Inputs:
    def __init__ ( self ):
        self.spec = json.loads(files("aegisx_model").joinpath("lifecycle.json").read_text())
        self.features = Features()
        self.content = Content()

    @staticmethod
    def bytes ( value ):
        if value is None: return b""
        if isinstance(value, bytes): return value
        if isinstance(value, str): return value.encode("utf-8")
        raise ValueError("Lifecycle text must be UTF-8 text or bytes")

    @staticmethod
    def tokens ( value, limit ):
        raw = Inputs.bytes(value)
        tokens = np.zeros(limit, dtype=np.int64)
        prefix = raw[:limit]
        tokens[:len(prefix)] = np.frombuffer(prefix, dtype=np.uint8).astype(np.int64) + 1
        return tokens

    @staticmethod
    def time ( value ):
        if value is None: value = 0
        if not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
            raise ValueError("Event time must be a finite nonnegative number")
        return math.log1p(min(value, 86400000)) / math.log1p(86400000)

    def record ( self, item ):
        request, response = self.bytes(item.get("request", "")), self.bytes(item.get("response", ""))
        events = item.get("events", [])
        text = np.stack([self.tokens(request, self.spec["text_bytes"]), self.tokens(response, self.spec["text_bytes"])])
        names = np.zeros((self.spec["event_count"], self.spec["event_bytes"]), dtype=np.int64)
        values = np.zeros((self.spec["event_count"], self.spec["event_values"]), dtype=np.float32)
        spans = {}
        for index, event in enumerate(events[:self.spec["event_count"]]):
            names[index] = self.tokens(event.get("service", "") + "\n" + event.get("operation", ""), self.spec["event_bytes"])
            parent = spans.get(event.get("parent_id"))
            values[index] = [1, event.get("state") == "started", event.get("state") == "completed",
                             event.get("state") == "failed", self.time(event.get("duration_ms")),
                             self.time(event.get("elapsed_ms")), (parent + 1) / self.spec["event_count"] if parent is not None else 0,
                             parent is not None]
            if event.get("span_id") and event.get("span_id") not in spans: spans[event["span_id"]] = index
        raw = self.content.row(request[:16384], len(request))
        if "admission" in item: raw[:16] = item["admission"]
        if "outcome" in item: raw[32:40] = item["outcome"]
        features = self.features.normalize([raw])[0]
        coverage = np.asarray([len(request) > self.spec["text_bytes"], len(response) > self.spec["text_bytes"],
                               len(events) > self.spec["event_count"] or item.get("events_truncated", False),
                               item.get("response_available", bool(response))], dtype=np.float32)
        return {"features": features, "text": text, "event_text": names, "event_values": values, "coverage": coverage}
