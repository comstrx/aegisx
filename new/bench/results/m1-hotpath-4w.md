# m1-hotpath-4w

workers=4 threads=2 seconds=12 trials=4 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 40513.3 | 2.23 | 31.715 | 64.93 | 21.64 | 43.29 | 0.196 | 31.7 | 0 |
| aegisx | 50102.4 | 1.955 | 16.88 | 64.89 | 28.48 | 36.41 | 0.088 | 23.1 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 65759.3 | 3.625 | 20.875 | 55.74 | 18.49 | 37.4 | 0.03 | 32.3 | 0 |
| aegisx | 64129.3 | 3.5 | 20.94 | 57.1 | 25.92 | 31.24 | 0.027 | 30.6 | 0 |
