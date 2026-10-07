# m4-final-2w

workers=2 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 27257.3 | 4.25 | 16.34 | 71.21 | 21.0 | 48.96 | 0.003 | 17.8 | 0 |
| aegisx | 30592.7 | 4.0 | 9.52 | 65.15 | 19.67 | 45.48 | 0.002 | 24.7 | 0 |
