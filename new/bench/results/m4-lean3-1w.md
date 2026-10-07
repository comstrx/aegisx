# m4-lean3-1w

workers=1 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 19728.6 | 6.24 | 10.16 | 49.97 | 14.33 | 35.81 | 0.001 | 10.8 | 0 |
| aegisx | 19770.4 | 6.3 | 8.9 | 50.49 | 13.3 | 37.57 | 0.002 | 24.0 | 0 |
