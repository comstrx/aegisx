# aws-r3-static-2w

mode=static protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 269853.3 | 0.39 | 0.737 | 7.39 | 3.14 | 4.25 | 0.001 | 17.3 | 0 |
| aegisx | 271356.0 | 0.343 | 0.92 | 7.34 | 3.46 | 3.86 | 0.001 | 30.3 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 256276.8 | 1.62 | 3.52 | 7.79 | 3.16 | 4.66 | 0.001 | 17.5 | 0 |
| aegisx | 250763.9 | 1.56 | 3.44 | 7.96 | 3.7 | 4.26 | 0.001 | 39.1 | 0 |
