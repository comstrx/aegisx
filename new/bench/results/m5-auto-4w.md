# m5-auto-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 58860.1 | 1.83 | 24.68 | 61.54 | 22.24 | 39.89 | 0.04 | 31.3 | 0 |
| aegisx | 60143.1 | 1.87 | 14.15 | 62.99 | 23.76 | 39.3 | 0.022 | 30.0 | 0 |
| h1only | 58376.6 | 1.88 | 14.83 | 62.64 | 23.76 | 38.86 | 0.032 | 30.5 | 0 |
