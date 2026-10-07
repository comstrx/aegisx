# aws-r5-cache-2w

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 198369.1 | 0.565 | 0.93 | 10.04 | 3.89 | 6.17 | 0.001 | 18.7 | 0 |
| aegisx | 277312.9 | 0.31 | 0.92 | 7.11 | 3.29 | 3.86 | 0.001 | 32.7 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 189815.6 | 2.24 | 4.07 | 10.52 | 3.83 | 6.69 | 0.001 | 22.1 | 0 |
| aegisx | 255574.6 | 1.67 | 3.65 | 7.81 | 3.58 | 4.23 | 0.001 | 43.5 | 0 |
