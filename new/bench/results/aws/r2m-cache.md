# r2m-cache

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 197352.1 | 0.544 | 0.94 | 10.13 | 3.8 | 6.31 | 0.001 | 17.6 | 0 |
| aegisx | 283506.7 | 0.363 | 0.736 | 7.03 | 3.02 | 3.98 | 0.001 | 34.1 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 193139.9 | 2.07 | 4.13 | 10.35 | 3.63 | 6.65 | 0.001 | 20.6 | 0 |
| aegisx | 264894.8 | 1.51 | 3.45 | 7.54 | 3.09 | 4.47 | 0.001 | 39.7 | 0 |
