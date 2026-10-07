# m3-notelemetry-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 60402.5 | 1.71 | 17.54 | 53.83 | 19.6 | 34.96 | 0.056 | 31.4 | 0 |
| aegisx | 63699.0 | 1.8 | 13.97 | 58.73 | 25.35 | 32.9 | 0.026 | 32.0 | 0 |
