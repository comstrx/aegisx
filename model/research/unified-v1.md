# Unified security research · 2026-10-02

The attached discussion changes the research direction toward **one jointly trained backbone** for text, observed numbers and reported execution events. It does not authorize inline inference, automatic deployment, invented ground truth, or calling a parameter count intelligence.

The shipped v0.9 executable and model remain unchanged. This round implements and actually runs a new research path; its first candidate fails the validation prerequisite.

## Decisions

1. Keep request completion as the analysis boundary. Explicit Lua policies and existing durable decisions remain the admission authority.
2. Use one network with modality-specific input projections and one shared pretrained transformer. Separate output targets are permitted when their labels answer different questions; they are not separate models voting.
3. Keep **public attack-content labels** distinct from **observed policy violations**. A failed attack attempt and a successful unauthorized write are different outcomes. Collapsing their labels into one binary target would introduce contradictions.
4. Generate scenarios, then execute them to obtain labels. Do not derive ground truth from another model's opinion. A rollback can undo a write but cannot undo private data already returned.
5. Treat encoding/field/route mutations as label-unknown until the transformed request is executed or equivalence is proven. The pasted discussion's blanket claim that transformations preserve the label is unsafe.
6. Separate training, checkpoint selection, calibration and final testing by groups. Do not mine errors from the final test set into training.
7. Use prediction-set abstention as an explicitly limited mechanism. It is not a trained unknown-attack classifier or a calibrated OOD probability. No family head is added without trustworthy family labels.
8. Compare raw proxy performance locally against Nginx before claiming competitive speed. [Actual measurements](../../server/benchmarks/nginx-comparison.md) show a material gap.
9. Reject an underperforming candidate before spending a new test set or replacing the packaged model. A small pilot cannot settle whether an architecture will work after adequate training.

The discussion's statement that v0.9 was unfinished was stale: v0.9 was complete before this request. Its complete evidence remains in [v0.9 verification](../../server/benchmarks/v09.md).

## Pretrained source

[ModernBERT-base](https://huggingface.co/answerdotai/ModernBERT-base) is an encoder with an Apache-2.0 license. We use official revision
`8949b909ec900327062f0ebf497f51aef5e6f0c8`, without remote repository code.

The 598,635,032-byte safetensors checkpoint is pinned to SHA-256
`340ac08b74eef0d7bdec2d7981a6a3d4249bf0e6aab60634b72ad02c2b8023a9`.
Tokenizer/config files are pinned by their revision/blob identity, then SHA-256 recorded and verified at load. See `modernbert-source.json`, `scripts/fetch_modernbert.py` and `data/modernbert/manifest.json`. Original model card and Apache license are retained with the downloaded artifact.

This is a credible candidate to test, not a claim that it is the universally strongest security model. Its original pretraining does not supply AegisX application-authorization knowledge automatically.

## Implemented input and network

`module/lifecycle/unified.py` projects request/response token embeddings, numeric groups, event names/states/timings/parents and coverage into the **same ModernBERT sequence**. One backbone produces one pooled representation and two supervised logits.

The local pilot deliberately limits token counts to 64 request tokens, 32 response tokens, 32 events with eight name tokens each, and five projected numeric/coverage tokens. These are tokenizer tokens from the existing bounded byte prefix. They do **not** preserve arbitrary full bodies or the model card's maximum context. The coverage input describes the original byte/event capture; tokenizer truncation is an additional fixed pilot limit. Enlarging context requires separate cost and quality measurements.

The existing 296-feature contract is retained as an input source. Features with no training support are masked. This experiment does not establish behavioral fraud learning, arbitrary code visibility, uploaded-file malware detection, or learned load balancing.

There is no new Rust inference contract for this candidate yet. The old embedded model remains the only running server model. Neither Transformers nor Python has been added as a production service.

## Executed data

The immutable round has **5,440 examples**:

- 4,096 selected examples from the verified v0.9 corpus, preserving its grouped split.
- **1,344 new executed SQLite counterfactuals**: authorized/unauthorized actors, early/late authorization, reads/writes, and commit/rollback.
- Legitimate text includes SQL documentation, script examples and traversal-like paths stored through parameterized SQL.
- Labels inspect actual committed effects and returned private data. Fixture controls, application IDs, labels and oracle state are excluded from model tensors.

Seven named fixture environments have disjoint train/selection/calibration/test membership. **They share the same generator and control-flow implementation.** This tests held-out schemas, service names and routes; it is not seven independently implemented applications or proof of unseen-production-app generalization.

Every tensor, label, group, task and partition is hashed. Complete post-tokenization inputs are checked for conflicting labels and cross-partition collisions; this run removed zero rows. Fingerprint:
`78cab457d1c2492c1052b607ccf09d2fc47cd7ad15c1bfd472f9a3dc569551c9`.

## Actual training

A new authorization record is separate from the already exhausted three-attempt v0.9 ledger. One new pilot ran **80 optimizer updates**, batch four, with balanced task/class sampling and an auxiliary loss on executable counterfactual pairs. This is 320 sampled presentations, including repeats, **not 80 epochs or training on every corpus row**.

| Stage | Updates | Trainable parameters | Pretrained parameters trainable |
| --- | ---: | ---: | ---: |
| Input projections/head and final norm | 16 | 173,186 | 768 |
| Joint inputs plus upper two transformer blocks | 48 | 10,203,266 | 10,030,848 |
| Entire network | 16 | **149,186,690** | **149,014,272** |

Training elapsed **504.4 seconds** on two CPU threads. A separate full-backward preflight took 18.31s for four samples and peaked around 2.06GiB RSS; that preflight did not perform an optimizer step. These numbers are not a production-inference benchmark.

Saved `selected.pt` contains the best validation checkpoint; `checkpoint.pt` contains final weights, optimizer state and random-generator state. The best selected checkpoint is from the **upper-two-block stage**, not the final full-network stage. The full-network stage worsened selection scores in this short experiment.

## Result and decision

Selection uses the same fixed 64-row subset per task, with 32 positives and 32 negatives. At the fitted low-FPR operating point, the best checkpoint detects:

| Selection task | True positives | False negatives | Observed false positives | Recall |
| --- | ---: | ---: | ---: | ---: |
| Public content | 10 | 22 | 0/32 | **31.25%** |
| Controlled journey | 16 | 16 | 0/32 | **50%** |

Thirty-two benign examples cannot establish a 1% population false-positive guarantee. These are checkpoint-selection results, not external results.

The research prerequisite is recall at least 90% with FPR at most 1%, followed by a comparison with the preserved baseline, independent application acceptance, and verified runtime/resource integration. This pilot fails the first prerequisite. **It is not promoted, exported into the production bundle or advertised as improved intelligence. Calibration and final-test prediction remain unperformed.**

This result says that the current short fine-tune is inadequate. It does not prove ModernBERT is unsuitable, that full fine-tuning is intrinsically bad, or that the proposed unified architecture is superior. Domain-adaptive pretraining on a licensed real HTTP corpus, broader independent application oracles and adequate training remain unimplemented research work.

## Reproduce and inspect

From the repository root, using the existing Python environment:

```sh
model/.venv/bin/python model/scripts/fetch_modernbert.py
model/.venv/bin/python model/scripts/research_unified.py prepare
model/.venv/bin/python model/scripts/research_unified.py preflight
model/.venv/bin/python model/scripts/research_unified.py train --steps 16 48 16
model/.venv/bin/python model/scripts/evaluate_unified.py
model/.venv/bin/python model/scripts/audit_unified.py
```

Preparation/training refuse to overwrite an existing round. Select a fresh `--data`/`--output` for a deliberately authorized new experiment. The existing run is already completed; the commands above describe its workflow, not a request to rerun training.

Evidence: `runs/unified-v1/data/manifest.json`, `candidate/authorization.json`, `candidate/history.json`, `candidate/training-report.json`, `candidate/assessment.json`, and `audit.json`. The evaluator stops before held-out scoring when the selection prerequisite fails. If it passes in a future candidate, calibration is fitted on a separate partition before final-test evaluation; no test-derived threshold adjustment is allowed.

All **32 Python tests** pass, including five new checks for executable disclosure/rollback labels, oracle exclusion, shared-backbone gradients, abstention and rejection before test inference. Ruff passes. No cloud job, data upload, payment, system service modification or Git write occurred.
