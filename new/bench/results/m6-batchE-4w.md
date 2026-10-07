# m6-batchE-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 46431.1 | 2.04 | 16.68 | 58.23 | 20.2 | 38.04 | 0.188 | 31.2 | 0 |
| aegisx | 33317.0 | 2.85 | 25.14 | 62.65 | 25.24 | 37.1 | 0.255 | 29.5 | 0 |
| access | 41066.9 | 2.42 | 19.53 | 68.6 | 29.82 | 40.29 | 0.17 | 40.3 | 0 |
