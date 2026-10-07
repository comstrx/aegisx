# m5-h1batch-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 43947.4 | 2.22 | 19.42 | 51.88 | 18.34 | 33.84 | 0.317 | 31.3 | 0 |
| aegisx | 40563.0 | 2.28 | 27.36 | 51.98 | 19.49 | 33.3 | 0.327 | 31.8 | 0 |
