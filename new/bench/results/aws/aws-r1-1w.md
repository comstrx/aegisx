# aws-r1-1w

workers=1 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 115508.5 | 1.09 | 1.4 | 8.64 | 3.24 | 5.4 | 0.001 | 11.3 | 0 |
| aegisx | 84007.6 | 1.51 | 1.66 | 11.9 | 5.88 | 5.95 | 0.002 | 31.2 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 102922.9 | 4.93 | 5.61 | 9.72 | 4.03 | 5.68 | 0.0 | 14.5 | 0 |
| aegisx | 63718.8 | 7.68 | 12.14 | 15.68 | 7.46 | 8.54 | 0.005 | 56.8 | 0 |
