# Unified ModernBERT-large experiment · 2026-10-02

The requested model above 200 million parameters was downloaded, verified and genuinely fine-tuned locally. **It is not approved to replace the embedded model.** This is one new bounded experiment under the new user request; earlier three-attempt rounds and unified-v1 remain unchanged.

## Architecture and source

One shared multimodal backbone, two supervised targets: request-content evidence and observed journey policy violations. Total **395,011,458 parameters**, including 394,781,696 pretrained backbone parameters and 229,762 new parameters. All parameters are trainable in the final stage.

Official [ModernBERT-large](https://huggingface.co/answerdotai/ModernBERT-large), Apache-2.0, pinned revision `45bb4654a4d5aaff24dd11d4781fa46d39bf8c13`. The 1,583,544,840-byte source checkpoint has SHA-256 `44510fec5d3a81a1877f225637b869495f18e55f6f23a09abb9be0acc030295f`. Every source file is verified against the saved manifest. No remote model code executes.

ModernBERT supplies pretrained language/code representations, not a pretrained API security detector. Request tokens: 128; response: 64; up to 32 ordered events with 16 name tokens each, numeric features and coverage indicators. This is bounded evidence, not unlimited request bodies or all backend code execution.

## Changes from the first pilot

- Small modality/projection initialization preserves the pretrained embedding scale.
- SDPA and removal of masked padding gaps reduce wasted attention work. Valid token order and positions are preserved; a regression test checks compact/noncompact parity.
- Token-budget truncation contributes to coverage inputs.
- Equal task/class sampling, paired safe/unsafe examples, a pair-ranking auxiliary loss, label smoothing and staged learning-rate warmup/cosine decay.
- AdamW for input/top-layer stages; memory-efficient [Adafactor](https://huggingface.co/docs/transformers/main_classes/optimizer_schedules) for the full network.
- Atomic checkpoint publication, RNG/optimizer/data-fingerprint resume checks, and finite-loss/gradient checks.

## Data

8,672 prepared examples, including 2,016 executed SQLite oracle fixtures. Sources reuse pinned HttpParams, FuzzDB, SecLists, WCP legitimate traffic and older controlled labs. New retry/idempotency fixtures compare committed database state: successful deduplication and rolled-back attempts remain safe; duplicate committed effects are violations. Paired cases can have identical HTTP request/response bodies, requiring journey evidence.

Fingerprint: `8ec3634e706d3fe6b207cdf1ab646f3880940c79e96c804b4cf82931317ef0c2`. Training, selection, calibration and final-test partitions are separate. Named application worlds and route/schema families are separated, but the controlled worlds share generator logic. This is not independent production-application validation. Public payload membership does not prove successful exploitation.

## Actual training and result

192 optimizer updates, batch 4: 32 input/head updates, 128 last-four-block updates, 32 full-network updates. **768 sample presentations, 652 distinct training rows**, not a full pass over the corpus. CPU only, two Torch threads; recorded wall time 2,730 seconds includes competing build work and deliberate benchmark pauses, so it is not a clean training-speed benchmark.

Selected checkpoint: final full-network stage. Audit confirms changes in the first attention and last MLP matrices; first/last sampled tensors have 3,145,639 and 5,373,932 changed values respectively. This is genuine backbone fine-tuning, not just counting frozen parameters.

| Selection target | Recall | Observed false positives |
| --- | ---: | ---: |
| Request content | 26/32 = 81.25% | 0/32 |
| Journey violations | 11/32 = 34.375% | 0/32 |

Both fail the predeclared 90% recall prerequisite. Zero errors among 32 benign rows cannot establish a 1% production FPR. Calibration and final-test model scoring remain unconsumed. The selected model was retained after workspace cleanup; the final optimizer checkpoint was removed. Original hashes and optimizer evidence remain in the audit report. Research artifacts are under `model/runs/unified-v2/candidate`; hashes and optimizer evidence are in `model/runs/unified-v2/audit.json`.

The 0.9 embedded UINT8 ONNX stays unchanged, SHA-256 `e00573a8e59dc131e3ad63e226be53a0690d200c443adbfe85ee27e1c24eb17a`. No 395M Rust/ONNX integration or quantization claim is made. Larger parameter count did not by itself establish adequate journey understanding. Further work requires substantially more diverse labeled execution evidence, training coverage and independent evaluation before deployment.

## Cleanup status

The selected.pt checkpoint and prepared unified-v2 data remain. The duplicate optimizer checkpoint and downloaded original backbone weights were removed during the user-requested 2026-10-02 cleanup. Restore pinned source weights and the Python environment before running research commands; audit.json records the original artifacts and cannot be rerun against a removed optimizer checkpoint.

## Reproduction

From the repository root, using `PYTHONPATH=model/src` and the existing model virtual environment:

```sh
model/.venv/bin/python model/scripts/research_unified.py train --data model/runs/unified-v2/data --pretrained model/data/modernbert-large --output NEW_CANDIDATE_DIRECTORY --steps 32 128 32
model/.venv/bin/python model/scripts/audit_unified.py --round model/runs/unified-v2 --pretrained model/data/modernbert-large
model/.venv/bin/python model/scripts/evaluate_unified.py --round model/runs/unified-v2 --pretrained model/data/modernbert-large
```

Never overwrite a completed experiment or use held-out test outcomes to tune its checkpoint. `--resume` is for interrupted, matching recipes; new experiments need new output directories.
