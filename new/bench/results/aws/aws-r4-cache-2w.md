# aws-r4-cache-2w

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 202329.2 | 0.552 | 1.0 | 9.85 | 3.72 | 6.13 | 0.001 | 18.5 | 0 |
| aegisx | 275775.1 | 0.356 | 0.782 | 7.23 | 3.36 | 3.88 | 0.001 | 31.4 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 190196.9 | 2.25 | 4.09 | 10.49 | 4.02 | 6.5 | 0.001 | 21.5 | 0 |
| aegisx | 253631.7 | 1.58 | 3.31 | 7.87 | 3.61 | 4.2 | 0.001 | 43.9 | 0 |
