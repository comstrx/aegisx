# m4-base-8w

workers=8 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 14791.3 | 5.01 | 59.75 | 93.2 | 34.95 | 58.25 | 0.682 | 59.4 | 0 |
| aegisx | 24080.0 | 3.54 | 46.55 | 105.63 | 46.98 | 59.64 | 0.362 | 38.8 | 0 |
