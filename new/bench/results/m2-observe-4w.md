# m2-observe-4w

workers=4 threads=2 seconds=12 trials=4 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 30406.5 | 3.095 | 32.97 | 61.79 | 21.66 | 40.16 | 0.322 | 31.6 | 0 |
| aegisx | 30691.3 | 2.95 | 37.655 | 65.06 | 29.11 | 35.45 | 0.283 | 23.4 | 0 |
