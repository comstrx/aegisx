# m3-quiet-4w

workers=4 threads=2 seconds=12 trials=4 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 53867.9 | 1.835 | 24.76 | 64.32 | 21.68 | 42.16 | 0.038 | 31.7 | 0 |
| aegisx | 46948.9 | 2.05 | 18.745 | 72.08 | 32.53 | 39.55 | 0.04 | 33.8 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 47847.5 | 4.16 | 36.18 | 70.56 | 23.65 | 46.92 | 0.047 | 32.7 | 0 |
| aegisx | 40412.8 | 4.635 | 40.08 | 85.18 | 38.44 | 46.74 | 0.026 | 41.9 | 0 |
