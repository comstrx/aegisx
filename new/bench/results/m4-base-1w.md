# m4-base-1w

workers=1 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 14804.0 | 7.67 | 20.7 | 67.46 | 21.22 | 44.94 | 0.002 | 10.9 | 0 |
| aegisx | 12078.3 | 9.66 | 27.1 | 79.95 | 32.04 | 47.91 | 0.006 | 27.5 | 0 |
