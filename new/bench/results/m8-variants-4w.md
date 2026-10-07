# m8-variants-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 59481.4 | 1.83 | 15.19 | 58.92 | 19.96 | 38.96 | 0.051 | 30.8 | 0 |
| nginx-log | 56349.9 | 1.9 | 22.93 | 62.27 | 22.18 | 40.71 | 0.036 | 30.9 | 0 |
| aegisx | 58853.4 | 1.9 | 13.12 | 62.58 | 23.56 | 39.01 | 0.022 | 29.8 | 0 |
| access | 56938.2 | 2.01 | 13.41 | 66.72 | 25.85 | 41.21 | 0.023 | 40.8 | 0 |
| compress | 59716.7 | 1.86 | 13.37 | 61.65 | 23.67 | 38.29 | 0.022 | 30.2 | 0 |
