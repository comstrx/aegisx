# m6-batchE2-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 67310.3 | 1.71 | 12.38 | 54.97 | 18.73 | 36.24 | 0.019 | 31.3 | 0 |
| aegisx | 50091.8 | 2.23 | 19.92 | 57.85 | 23.13 | 34.86 | 0.121 | 29.4 | 0 |
| access | 57714.4 | 2.05 | 11.79 | 67.72 | 28.21 | 39.4 | 0.016 | 38.6 | 0 |
| compress | 63535.2 | 1.86 | 12.96 | 59.27 | 22.81 | 36.58 | 0.023 | 31.9 | 0 |
