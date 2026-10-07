# aws-r4-1w

mode=proxy protocol=h1 workers=1 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 109338.1 | 1.16 | 1.48 | 9.14 | 3.29 | 5.74 | 0.001 | 11.3 | 0 |
| aegisx | 82309.9 | 1.54 | 1.96 | 12.15 | 6.37 | 6.37 | 0.002 | 32.6 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 97648.1 | 5.09 | 5.95 | 10.24 | 3.91 | 6.41 | 0.001 | 14.8 | 0 |
| aegisx | 72237.3 | 6.88 | 9.79 | 13.82 | 7.64 | 6.19 | 0.002 | 62.1 | 0 |
