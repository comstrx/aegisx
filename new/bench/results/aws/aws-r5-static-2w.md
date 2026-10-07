# aws-r5-static-2w

mode=static protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 266792.3 | 0.382 | 0.782 | 7.33 | 3.14 | 4.19 | 0.001 | 17.3 | 0 |
| aegisx | 268201.3 | 0.429 | 0.95 | 7.42 | 3.52 | 3.9 | 0.001 | 30.5 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 252399.4 | 1.67 | 3.58 | 7.91 | 3.17 | 4.69 | 0.001 | 17.5 | 0 |
| aegisx | 249070.4 | 1.61 | 3.39 | 8.02 | 3.81 | 4.25 | 0.001 | 41.8 | 0 |
