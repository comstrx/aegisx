# m8-cache-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 89343.1 | 1.21 | 7.49 | 40.85 | 16.29 | 24.56 | 0.009 | 31.3 | 0 |
| aegisx | 100275.4 | 0.713 | 13.6 | 28.23 | 13.74 | 14.5 | 0.051 | 30.8 | 0 |
