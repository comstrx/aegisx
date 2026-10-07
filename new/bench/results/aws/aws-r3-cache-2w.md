# aws-r3-cache-2w

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 200328.0 | 0.497 | 1.03 | 9.96 | 3.74 | 6.22 | 0.001 | 18.5 | 0 |
| aegisx | 280858.3 | 0.378 | 0.747 | 7.11 | 3.33 | 3.85 | 0.001 | 29.8 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 188119.0 | 2.2 | 3.98 | 10.62 | 4.01 | 6.53 | 0.001 | 19.6 | 0 |
| aegisx | 251053.7 | 1.48 | 3.58 | 7.95 | 3.64 | 4.31 | 0.001 | 43.6 | 0 |
