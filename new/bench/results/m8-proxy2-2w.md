# m8-proxy2-2w

workers=2 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 21980.3 | 4.56 | 33.72 | 63.81 | 20.56 | 42.92 | 0.133 | 17.6 | 0 |
| aegisx | 21965.8 | 4.63 | 29.45 | 64.56 | 24.84 | 39.95 | 0.147 | 28.8 | 0 |
