# aws-r4-static-2w

mode=static protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 265388.3 | 0.414 | 0.733 | 7.52 | 3.21 | 4.33 | 0.001 | 17.4 | 0 |
| aegisx | 271268.7 | 0.373 | 0.94 | 7.35 | 3.47 | 3.88 | 0.001 | 33.2 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 252569.6 | 1.65 | 3.15 | 7.91 | 3.14 | 4.74 | 0.001 | 17.6 | 0 |
| aegisx | 249420.1 | 1.58 | 3.5 | 8.01 | 3.73 | 4.3 | 0.001 | 43.7 | 0 |
