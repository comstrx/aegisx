import math
import numpy as np

from .metrics import Metrics

import torch
from torch import nn

from ..core import ModelError


class Fit:

    @staticmethod
    def scores ( network, values, batch_size, device ):

        network.eval()
        with torch.no_grad():
            return torch.cat([torch.sigmoid(network(torch.from_numpy(values[start:start + batch_size]).to(device))).cpu()
                              for start in range(0, len(values), batch_size)]).numpy().reshape(-1)

    @staticmethod
    def run ( network, optimizer, checkpoint, data, train, validation, epochs, seed, options, max_fpr=.01 ):

        values = torch.from_numpy(data.values[train])
        labels = torch.from_numpy(data.labels[train])
        weight = float((labels == 0).sum() / (labels == 1).sum()) if options.balance else 1.0
        loss_function = nn.BCEWithLogitsLoss(pos_weight=torch.tensor([weight], device=options.device), reduction="none")
        weights = torch.ones_like(labels)
        origins = np.asarray(getattr(data, "origins", ["default"] * len(data.labels)))[train]
        if options.source_balance:
            families = np.asarray([value.removesuffix("_query").removesuffix("_json") for value in origins])
            for label in (0, 1):
                selected = data.labels[train].reshape(-1) == label
                names, counts = np.unique(families[selected], return_counts=True)
                for name, count in zip(names, counts, strict=True):
                    weights[selected & (families == name)] = min(4.0, max(.25, selected.sum() / len(names) / count))
        validation_loss = nn.BCEWithLogitsLoss(reduction="sum")
        valid_values, valid_labels = torch.from_numpy(data.values[validation]), torch.from_numpy(data.labels[validation])
        for epoch in range(checkpoint.epoch, epochs):
            if options.patience and checkpoint.stale >= options.patience: break
            network.train()
            generator = torch.Generator().manual_seed(seed + epoch)
            order = torch.randperm(len(values), generator=generator)
            for indices in order.split(options.batch_size):
                optimizer.zero_grad(set_to_none=True)
                loss = (loss_function(network(values[indices].to(options.device)), labels[indices].to(options.device)) * weights[indices].to(options.device)).mean()
                if not torch.isfinite(loss): raise ModelError("Training produced a non-finite loss")
                loss.backward()
                nn.utils.clip_grad_norm_(network.parameters(), options.clip_norm, error_if_nonfinite=True)
                optimizer.step()
            network.eval()
            total, predictions = 0.0, []
            with torch.no_grad():
                for start in range(0, len(valid_values), options.batch_size):
                    batch = valid_values[start:start + options.batch_size].to(options.device)
                    truth = valid_labels[start:start + options.batch_size].to(options.device)
                    logits = network(batch)
                    total += validation_loss(logits, truth).item()
                    predictions.append(torch.sigmoid(logits).cpu().numpy().reshape(-1))
            loss = total / len(valid_values)
            if not math.isfinite(loss): raise ModelError("Validation produced a non-finite loss")
            criterion = loss
            if options.selection == "recall":
                scores = np.concatenate(predictions)
                truth = data.labels[validation].reshape(-1)
                threshold = Metrics.calibrate(truth, scores, max_fpr)
                families = np.asarray(getattr(data, "origins", ["default"] * len(data.labels)))[validation]
                recalls = [float((scores[(families == name) & (truth == 1)] >= threshold).mean())
                           for name in np.unique(families[truth == 1])]
                criterion = 1 - float(np.mean(recalls)) + min(loss, 100) * .0001
            checkpoint.observe(network, criterion, epoch + 1, options.min_delta)
            print(f"epoch={epoch + 1} validation_loss={loss:.6f} selection={criterion:.6f} best={checkpoint.best_epoch}", flush=True)
            if (epoch + 1) % options.checkpoint_every == 0: checkpoint.save(network, optimizer)
        checkpoint.save(network, optimizer)
        return weight
