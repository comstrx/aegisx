# m8-proxy-1w

workers=1 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 14366.7 | 8.03 | 16.65 | 66.35 | 19.76 | 48.52 | 0.025 | 10.6 | 0 |
| aegisx | 15670.6 | 7.71 | 12.85 | 63.9 | 18.92 | 44.05 | 0.002 | 24.6 | 0 |
