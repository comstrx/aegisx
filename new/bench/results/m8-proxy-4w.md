# m8-proxy-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 53441.1 | 1.87 | 18.43 | 57.52 | 19.57 | 38.01 | 0.118 | 31.3 | 0 |
| aegisx | 63616.5 | 1.89 | 13.64 | 59.97 | 23.07 | 37.14 | 0.023 | 35.0 | 0 |
