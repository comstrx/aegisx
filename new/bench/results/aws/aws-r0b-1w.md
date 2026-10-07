# aws-r0b-1w

workers=1 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 113348.9 | 1.12 | 1.38 | 8.82 | 3.07 | 5.88 | 0.001 | 10.5 | 0 |
| aegisx | 82209.6 | 1.54 | 1.78 | 12.16 | 6.65 | 5.51 | 0.001 | 33.9 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 98465.3 | 5.06 | 6.14 | 10.15 | 3.57 | 6.4 | 0.001 | 13.4 | 0 |
| aegisx | 48471.5 | 10.02 | 16.65 | 20.55 | 8.9 | 11.47 | 0.005 | 57.7 | 0 |
