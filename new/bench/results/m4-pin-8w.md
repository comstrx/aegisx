# m4-pin-8w

workers=8 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 71509.5 | 1.27 | 13.11 | 53.74 | 20.92 | 32.82 | 0.258 | 59.4 | 0 |
| aegisx | 52784.0 | 1.52 | 12.39 | 65.65 | 29.26 | 36.38 | 0.704 | 36.4 | 0 |
| nopin | 55963.0 | 1.49 | 17.49 | 58.79 | 24.11 | 32.15 | 0.31 | 38.0 | 0 |
