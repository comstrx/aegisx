# AegisX model · 0.10 research / 0.9 embedded artifact

Workspace cleanup removed older checkpoint/export/tensor binaries and the Python environment. Historical reports and recorded oracle examples remain; only unified-v2 selected.pt and its prepared data are retained as large research artifacts. The production weights directory is unchanged. See the root README and cleanup-report.json before reproducing archived experiments.

**125.64M-parameter transfer teacher; 895,178-parameter embedded student.** Three authorized candidates were actually trained. The selected student processes bounded request/response bytes, ordered backend reports and 296 numeric features, entirely in the background.

The teacher is Microsoft CodeBERT with **124,645,632 frozen pretrained parameters** plus **995,617 newly trained head parameters**. We did **not** retrain 125M parameters from scratch. Its encoder is training-only; Rust runs the compact student. The selected UINT8 ONNX is **1,224,330 bytes**, compared with 11,229,847 bytes in 0.8. All student parameters are trained.

This remains **research/observe only**, with `deployment_ready=false`. Internal improvement does not establish broad attack coverage. External CRS detection remains weak, and neither zero false positives nor zero missed attacks is guaranteed.

The subsequent [unified ModernBERT research path](research/unified-v1.md) is implemented and has a completed, rejected pilot. It leaves this shipped model unchanged.

A new [395M-parameter full-backbone experiment](research/unified-v2.md) completed 192 updates with expanded execution oracles. It did not meet the validation prerequisite and is **not shipped**. The model described below remains the embedded runtime artifact.

## What changed

- A pretrained request-content teacher, followed by a supervised compact student and a distilled student on the same grouped corpus.
- Original labels remain authoritative. Distillation uses only training rows, a temperature of 2 and a small auxiliary loss; confidently wrong teacher targets are excluded.
- A dilated byte encoder and smaller fusion layers replace the large dense bottleneck. Both text streams, reported event order/parents/timing and numeric features remain real inputs.
- Symmetric label smoothing reduces saturated probabilities. Checkpoint selection combines recall at fixed 0.1% and 1% validation FPR budgets, with balanced loss as a tie breaker.
- 3,072 additional **executed** SQLite transaction contrasts cover authorization, negative/excessive transfers, commit and rollback. Labels come from committed state.
- Corpus fingerprints now cover every tensor file, labels, groups, origins, schema and provenance. Edited labels or grouping cannot silently reuse a checkpoint.

## Data and its limits

29,373 rows: HttpParams 8,000; FuzzDB 944; SecLists 4,096; WCP publisher-labeled benign HTTP 9,165; recorded SQLite labs 7,168. This is a curated, bounded experiment, not millions of independent observations.

Sources and licenses are pinned in `sources.json` and source manifests:

- [HttpParamsDataset](https://github.com/Morzeux/HttpParamsDataset): public request-payload labels.
- [FuzzDB](https://github.com/fuzzdb-project/fuzzdb) and [SecLists](https://github.com/danielmiessler/SecLists): attack-oriented payload corpora. Inclusion is not proof that every string exploits every application.
- [WAF Comparison Project](https://github.com/openappsec/waf-comparison-project): publisher-labeled benign URI/body samples. Credentials and headers are excluded.
- [Microsoft CodeBERT](https://huggingface.co/microsoft/codebert-base), revision `3b0952feddeffad0063f274080e3c23d75e7eb39`: pretrained programming/natural-language representations, not an off-the-shelf security detector. Checkpoint SHA-256 is `28b61fd8fa069f6bc966f4cb9572a4026ab2a784fca8fb224020d91b744e32d6`.
- [OWASP CRS](https://github.com/coreruleset/coreruleset): **diagnostics only**, never added to this round's training. Expected rule triggers are positive proxies, not verified exploit outcomes.

The teacher uses 6,000 training and 1,500 validation request examples, selected without model scores; 1,500 test examples are held for reporting. Encoding uses at most 128 tokenizer tokens from the bounded request prefix. No teacher journey-understanding claim is made.

Public payloads have no fabricated responses or middleware events. The older lab executes safe/unsafe SQL reads and authorization-before/after-write. The new transaction lab executes real SQLite statements and compares final balances against policy. Fixture controls, labels and source names never become model inputs.

Capture owners, canonical templates and identical complete inputs are connected before the deterministic group split. Contradictory exact inputs are removed. Source ownership can form large connected components, so split sizes are not exactly 60/20/20. Generated route families are separated, but shared fixture logic is still shared: this is not independent production-application validation.

## Measured quality

Thresholds are fitted only on validation. Candidate selection was saved before final test inspection. Zero observed false positives below describes finite samples.

| Held-out test, 0.1% validation FPR budget | 0.8 baseline | Supervised student | Selected distilled student |
| --- | ---: | ---: | ---: |
| Content recall, 1,800 positives | 81.22% | 97.67% | **98.39%** |
| Content false positives, 975 benign | 0 | 0 | 0 |
| Journey recall, 341 lab violations | See note | 100% | **95.31%** |
| Journey false positives, 1,083 safe lab rows | See note | 0 | 0 |

The old journey model could not satisfy either FPR budget on the expanded validation fixture: even threshold 1 produced 149/1,074 false positives. Its threshold-1 test has 71/341 true positives and 109/1,083 false positives; it is **not** a matched-budget result. On the original lab at its original shipped threshold, 0.8 previously measured 93.07% recall. The selected student's original-lab recall is also 93.07%; its new transaction subset detects 110/110 violations with 0/418 false positives. The supervised student's better final journey test was not used to revise the validation-frozen selection.

The teacher's matched 1,500-row request test at its strict validation threshold: **99.2% recall**, **1/750 false positives**. This exceeds the empirical 0.1% validation budget on test and illustrates why a validation constraint is not a guarantee. Zeroing pretrained embeddings drops recall to 56.8% at that threshold; zeroing numeric features gives 98.93%. These ablations alter the input distribution and only demonstrate dependence on the learned representation.

| Previously inspected external diagnostics | 0.8 baseline | Selected student |
| --- | ---: | ---: |
| WCP benign FPR, strict 0.1% validation budget | 3/7,491 = 0.040% | 7/7,491 = **0.093%** |
| CRS positive recall, same strict budget | 137/2,822 = 4.85% | 261/2,822 = **9.25%** |
| WCP benign FPR, 1% validation budget | 0.561% | 0.748% |
| CRS positive recall, same 1% budget | 16.12% | 26.26% |

External recall improves at matched validation budgets **with more false positives**. At the unchanged numerical threshold 0.95, the new artifact instead has 0.080% WCP FPR and 9.07% CRS recall, versus old 1.976% / 28.70%; raw probabilities are not comparable calibrations across architectures. These diagnostics were inspected in earlier rounds, and must not be presented as pristine acceptance tests. No external threshold fitting or fourth training attempt occurred.

Selected strict thresholds: content `0.9491480588912964`, journey `0.9946383237838745`. Alternative 1% content threshold: `0.723823070526123`. Configure the applicable value in Lua; artifact recommendations do not silently override user policy.

## Input and runtime contract

The `byte-journey-v1` contract remains:

| Input | Shape | Meaning |
| --- | --- | --- |
| features | [1,296] float32 | normalized observed numeric facts |
| text | [1,2,1024] int64 | request/response bytes, byte+1; zero padding |
| event_text | [1,32,64] int64 | reported service + newline + operation |
| event_values | [1,32,8] float32 | presence/state, bounded log timing, first earlier parent index/known flag |
| coverage | [1,4] float32 | request/response/event truncation and response availability |

The content head uses request bytes and observed content features. The journey head uses both streams, events and available numeric facts. No request text or events means that head is masked to zero; Rust also checks availability before applying its threshold. Outputs are `risk=max(content_risk,journey_risk)`; Rust checks shape, range, finiteness and this relation.

No raw text is persisted. Request prefix collection is bounded; response text requires explicit opt-in. The completed envelope is normalized in the worker. Byte tensors preserve original text while numeric extraction uses bounded double URL decoding. Interrupted text jobs cannot be reconstructed from the numeric-only journal and become `recovery_unavailable`. Legacy numeric artifacts retain observation-only compatible recovery.

History is observed behavior, not previous model predictions. Unsupported admission/history dimensions remain masked in the journey model. This round does not establish tenant fraud, upload malware classification, arbitrary uninstrumented middleware visibility, or learned infrastructure control.

## Evidence and reproduction

- `runs/v09/attempts.json`: three consumed attempts, real status and timings.
- `runs/v09/data/manifest.json`: corpus fingerprints and provenance.
- `runs/v09/attempt-{1,2,3}/`: trained states, optimizer checkpoints for students, history and exports.
- `runs/v09/selection.json`: validation-frozen choice; `evaluation.json`: final tests.
- `weights/v09-external-evaluation.json`, `weights/v09-teacher-ablation.json`.
- `weights/report.json`: selected training/export report; `weights/metadata.json`: shipped identity and thresholds.
- [Server verification](../server/benchmarks/v09.md): actual binary, throughput and integration checks.

On this shared WSL CPU, frozen teacher embedding extraction took 578.9s and head training 17.8s. Supervised training took 494.4s (13 epochs, best 8); distillation 380.3s (16 epochs, best 11). Some work overlapped server compilation; these are elapsed experiment times, not comparative training benchmarks. No cloud purchase, job or data upload occurred.

Use the pinned Python environment and `uv sync --group dev`. The reproducible workflow is `fetch_teacher.py`, `prepare_v09.py`, `prepare_teacher.py`, three registered `train_v09.py --attempt N` invocations, export/validation selection, external diagnostics and promotion. **The saved round's three-attempt budget is exhausted.** Training scripts refuse to overwrite consumed attempts. A new round requires a fresh authorization/ledger and output directory.

Run `uv run pytest`, `uv run ruff check src scripts tests`, and `uv run python scripts/audit_lifecycle.py` for verification without retraining. PyTorch/ONNX and Python/Rust parity include values near deployed thresholds. Quantization is promoted only when smaller, measured faster and within the configured validation-recall tolerance; convolutions/embeddings remain floating point. See [ONNX Runtime quantization guidance](https://onnxruntime.ai/docs/performance/model-optimizations/quantization.html).

The teacher's MIT and third-party notices are retained in `weights/CODEBERT-NOTICE.txt`. Transformers is a development dependency only. Training checkpoint loading uses weights-only loading and the fixed local official architecture.
