# aws-r0-1w

workers=1 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 109607.1 | 1.13 | 1.47 | 9.12 | 3.46 | 5.76 | 0.001 | 10.8 | 0 |
| aegisx | 81883.6 | 1.53 | 1.96 | 12.19 | 6.1 | 6.11 | 0.001 | 33.0 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 101103.1 | 5.04 | 5.84 | 9.88 | 3.9 | 6.12 | 0.001 | 14.4 | 0 |
| aegisx | 53517.4 | 8.5 | 14.65 | 18.63 | 8.7 | 9.72 | 0.004 | 55.3 | 0 |
