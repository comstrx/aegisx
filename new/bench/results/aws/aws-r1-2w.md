# aws-r1-2w

workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 152760.5 | 0.593 | 2.46 | 12.91 | 4.26 | 8.6 | 0.003 | 18.4 | 0 |
| aegisx | 113402.6 | 1.08 | 2.55 | 17.61 | 8.64 | 8.92 | 0.001 | 32.3 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 139391.1 | 2.84 | 6.05 | 14.3 | 4.79 | 9.68 | 0.003 | 21.6 | 0 |
| aegisx | 105933.8 | 3.82 | 9.84 | 18.84 | 9.15 | 9.68 | 0.003 | 63.0 | 0 |
