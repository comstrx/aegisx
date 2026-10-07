# aws-base-1w

workers=1 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 111746.2 | 1.12 | 1.43 | 8.95 | 3.39 | 5.55 | 0.001 | 10.8 | 0 |
| aegisx | 108459.9 | 1.17 | 1.45 | 9.2 | 3.89 | 5.32 | 0.001 | 31.5 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 101220.9 | 5.02 | 5.92 | 9.88 | 3.73 | 6.12 | 0.0 | 14.4 | 0 |
| aegisx | 95028.2 | 5.25 | 9.62 | 10.53 | 4.42 | 6.23 | 0.001 | 57.1 | 0 |
