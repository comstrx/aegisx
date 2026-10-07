"""One pretrained backbone fuses all modalities; task outputs share its representation."""
import hashlib
import json

import torch
from torch import nn
from transformers import AutoModel


class Unified:
    @staticmethod
    def load ( directory, compact=False ):
        manifest = json.loads((directory / "manifest.json").read_text())
        for name, expected in manifest["verified"].items():
            path = directory / name
            with path.open("rb") as stream: digest = hashlib.file_digest(stream, "sha256").hexdigest()
            if path.stat().st_size != expected["bytes"] or digest != expected["sha256"]:
                raise ValueError("Pretrained artifact integrity mismatch")
        return UnifiedNetwork(AutoModel.from_pretrained(str(directory), local_files_only=True,
                              trust_remote_code=False, attn_implementation="sdpa"), compact=compact)


class UnifiedNetwork(nn.Module):
    def __init__ ( self, backbone, compact=False ):
        super().__init__()
        self.backbone = backbone
        self.compact = compact
        width = backbone.config.hidden_size
        self.types = nn.Embedding(5, width)
        self.numeric = nn.Linear(74, width)
        self.event_values = nn.Linear(8, width)
        self.coverage = nn.Linear(4, width)
        self.head = nn.Sequential(nn.LayerNorm(width), nn.Linear(width, 128), nn.GELU(), nn.Linear(128, 2))
        self.register_buffer("supported_features", torch.ones(296))
        # Added modalities must not swamp the pretrained token embedding scale.
        nn.init.normal_(self.types.weight, std=.002)
        for projection in (self.numeric, self.event_values, self.coverage):
            nn.init.normal_(projection.weight, std=.02 / projection.in_features**.5)
            nn.init.zeros_(projection.bias)

    def trainable ( self, upper_layers=None ):
        for parameter in self.parameters(): parameter.requires_grad_(True)
        if upper_layers is not None:
            for parameter in self.backbone.parameters(): parameter.requires_grad_(False)
            for layer in self.backbone.layers[-upper_layers:] if upper_layers else []:
                for parameter in layer.parameters(): parameter.requires_grad_(True)
            for parameter in self.backbone.final_norm.parameters(): parameter.requires_grad_(True)
        return {"total_parameters": sum(p.numel() for p in self.parameters()),
                "trainable_parameters": sum(p.numel() for p in self.parameters() if p.requires_grad),
                "pretrained_trainable_parameters": sum(p.numel() for p in self.backbone.parameters() if p.requires_grad)}

    def forward ( self, request_ids, request_mask, response_ids, response_mask,
                  event_ids, event_mask, event_values, features, coverage ):
        embedding = self.backbone.get_input_embeddings()
        request = embedding(request_ids) + self.types.weight[0]
        response = embedding(response_ids) + self.types.weight[1]
        present = event_mask.unsqueeze(-1)
        events = (embedding(event_ids) * present).sum(2) / present.sum(2).clamp(min=1)
        events = events + self.event_values(event_values) + self.types.weight[2]
        numeric = self.numeric((features * self.supported_features).reshape(-1, 4, 74)) + self.types.weight[3]
        cover = self.coverage(coverage).unsqueeze(1) + self.types.weight[4]
        inputs = torch.cat((request, response, events, numeric, cover), dim=1)
        mask = torch.cat((request_mask, response_mask, event_values[:, :, 0],
                          torch.ones(features.shape[0], 5, device=features.device)), dim=1).long()
        if self.compact:
            # Preserve token order while removing padding holes. Each example then has
            # the same positions regardless of its neighbors in the training batch.
            counts = mask.sum(1)
            length = int(counts.max().item())
            positions = torch.arange(mask.shape[1], device=mask.device).unsqueeze(0)
            order = (positions + (1-mask)*mask.shape[1]).argsort(dim=1)[:, :length]
            inputs = inputs.gather(1, order.unsqueeze(-1).expand(-1, -1, inputs.shape[-1]))
            mask = (torch.arange(length, device=mask.device).unsqueeze(0) < counts.unsqueeze(1)).long()
        encoded = self.backbone(inputs_embeds=inputs, attention_mask=mask).last_hidden_state
        representation = (encoded * mask.unsqueeze(-1)).sum(1) / mask.sum(1, keepdim=True).clamp(min=1)
        # These are output tasks, not separate models: public attack-content labels and executed policy outcomes differ.
        return self.head(representation)
