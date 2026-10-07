"""Bounded byte sequences, ordered backend events and numeric context."""
import json
from importlib.resources import files

import torch
from torch import nn
from torch.nn import functional as F


class LifecycleNetwork ( nn.Module ):
    def __init__ ( self, variant="legacy" ):
        super().__init__()
        self.spec = json.loads(files("aegisx_model").joinpath("lifecycle.json").read_text())
        self.variant = variant
        self.text_embedding = nn.Embedding(257, 32, padding_idx=0)
        self.text_layers = nn.Sequential(nn.Conv1d(32, 64, 5, stride=4, padding=2), nn.ReLU(),
                                         nn.Conv1d(64, 96, 5, stride=4, padding=2), nn.ReLU())
        self.event_embedding = nn.Embedding(257, 16, padding_idx=0)
        self.event_project = nn.Linear(24, 64)
        self.event_layers = nn.Sequential(nn.Conv1d(64, 64, 3, padding=1), nn.ReLU())
        self.register_buffer("feature_mask", torch.ones(296))
        self.content_head = nn.Sequential(nn.Linear(464, 256), nn.ReLU(), nn.Dropout(.1), nn.Linear(256, 1))
        self.normalization = nn.LayerNorm(876)
        self.fusion = nn.Sequential(nn.Linear(876, 2048), nn.ReLU(), nn.Dropout(.2),
                                    nn.Linear(2048, 4096), nn.ReLU(), nn.Dropout(.2),
                                    nn.Linear(4096, 128), nn.ReLU(), nn.Linear(128, 1))

        if variant == "compact":
            # Two receptive-field scales retain short operators and longer byte structure.
            self.text_layers = nn.Sequential(nn.Conv1d(32, 64, 5, stride=4, padding=2), nn.ReLU(),
                nn.Conv1d(64, 64, 3, padding=2, dilation=2, groups=64), nn.ReLU(),
                nn.Conv1d(64, 96, 5, stride=4, padding=2), nn.ReLU())
            self.content_head = nn.Sequential(nn.LayerNorm(464), nn.Linear(464, 384), nn.ReLU(),
                nn.Dropout(.15), nn.Linear(384, 128), nn.ReLU(), nn.Linear(128, 1))
            self.fusion = nn.Sequential(nn.Linear(876, 512), nn.ReLU(), nn.Dropout(.15),
                nn.Linear(512, 256), nn.ReLU(), nn.Dropout(.1), nn.Linear(256, 64), nn.ReLU(), nn.Linear(64, 1))
            self.spec = self.spec | {"architecture": "dilated byte CNN + ordered event CNN + compact numeric fusion",
                                    "fusion": [876, 512, 256, 64, 1]}
        elif variant == "legacy":
            self.spec = self.spec | {"architecture": "shared byte CNN + ordered event CNN + numeric fusion",
                                    "fusion": [876, 2048, 4096, 128, 1]}
        else:
            raise ValueError("Unknown lifecycle network variant")
        self.spec["parameter_count"] = self.parameters_count()
        self.spec["compatible_architectures"] = [
            {"model_version": "lifecycle-v8", "parameter_count": 10901802},
            {"model_version": "lifecycle-v9", "parameter_count": 895178},
        ]

    @staticmethod
    def pool ( values, mask ):
        total = (values * mask).sum(-1) / mask.sum(-1).clamp(min=1)
        maximum = values.masked_fill(mask == 0, -1e4).amax(-1)
        return torch.cat((total, maximum * (mask.sum(-1) > 0)), dim=-1)

    def encode_text ( self, tokens ):
        tokens = tokens.reshape(-1, self.spec["text_bytes"])
        mask = (tokens != 0).float().unsqueeze(1)
        encoded = self.text_layers(self.text_embedding(tokens).transpose(1, 2))
        mask = F.max_pool1d(mask, 5, 4, 2)
        if self.variant == "compact": mask = F.max_pool1d(F.pad(mask, (2, 2)), 3, 1, 0, dilation=2)
        mask = F.max_pool1d(mask, 5, 4, 2)
        return self.pool(encoded, mask)

    def content_logits ( self, features, text ):
        encoded = self.encode_text(text[:, 0])
        content_features = torch.cat((features[:, 16:32], features[:, 40:]), dim=-1)
        return self.content_head(torch.cat((encoded, content_features), dim=-1))

    def journey_logits ( self, features, text, event_text, event_values, coverage ):
        batch = features.shape[0]
        features = features * self.feature_mask
        content = self.encode_text(text).reshape(batch, 384)
        event_mask = (event_text != 0).float().unsqueeze(-1)
        names = (self.event_embedding(event_text) * event_mask).sum(2) / event_mask.sum(2).clamp(min=1)
        events = F.relu(self.event_project(torch.cat((names, event_values), dim=-1)))
        valid = event_values[:, :, :1].transpose(1, 2)
        sequence = self.event_layers((events * valid.transpose(1, 2)).transpose(1, 2))
        pooled = self.pool(sequence, valid)
        # A positional moment preserves coarse ordering beyond the local CNN window.
        position = torch.arange(1, self.spec["event_count"] + 1, device=features.device).float() / self.spec["event_count"]
        ordered = (events.transpose(1, 2) * valid * position).sum(-1) / valid.sum(-1).clamp(min=1)
        joined = torch.cat((content, pooled, ordered, features, coverage), dim=-1)
        return self.fusion(self.normalization(joined))

    def forward ( self, features, text, event_text, event_values, coverage ):
        content = torch.sigmoid(self.content_logits(features, text)) * (text[:, 0].sum(-1, keepdim=True) > 0)
        journey = torch.sigmoid(self.journey_logits(features, text, event_text, event_values, coverage))
        journey = journey * (event_values[:, :, 0].sum(-1, keepdim=True) > 0)
        return torch.maximum(content, journey), content, journey

    def parameters_count ( self ):
        return sum(value.numel() for value in self.parameters())
