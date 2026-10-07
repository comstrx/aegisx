# m3-quiet-8w

workers=8 threads=2 seconds=12 trials=4 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 16932.5 | 4.25 | 65.87 | 84.83 | 32.74 | 54.41 | 0.622 | 59.4 | 0 |
| aegisx | 23447.8 | 5.09 | 74.57 | 98.13 | 45.15 | 52.98 | 0.326 | 37.0 | 0 |
