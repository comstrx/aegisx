# m4-lean-4w

workers=4 threads=2 seconds=8 trials=4 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 51099.6 | 1.905 | 21.01 | 58.31 | 20.54 | 37.62 | 0.095 | 31.5 | 0 |
| aegisx | 47223.9 | 1.985 | 24.71 | 62.86 | 25.12 | 38.23 | 0.076 | 31.4 | 0 |
