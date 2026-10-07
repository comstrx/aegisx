# aws-r4-2w

mode=proxy protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 149175.9 | 0.601 | 1.8 | 13.1 | 4.28 | 8.74 | 0.003 | 18.4 | 0 |
| aegisx | 112433.6 | 1.16 | 2.8 | 17.77 | 8.6 | 9.25 | 0.002 | 35.3 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 133653.0 | 2.7 | 6.51 | 14.92 | 4.97 | 9.95 | 0.003 | 21.4 | 0 |
| aegisx | 106817.3 | 3.66 | 10.19 | 18.68 | 9.02 | 9.84 | 0.002 | 65.5 | 0 |
