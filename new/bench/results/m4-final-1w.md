# m4-final-1w

workers=1 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 17471.0 | 7.08 | 12.15 | 55.6 | 14.41 | 41.5 | 0.003 | 10.8 | 0 |
| aegisx | 17262.1 | 7.2 | 12.3 | 57.19 | 16.49 | 41.94 | 0.002 | 23.6 | 0 |
