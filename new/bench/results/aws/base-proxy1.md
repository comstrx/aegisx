# base-proxy1

mode=proxy protocol=h1 workers=1 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 114447.4 | 1.09 | 1.5 | 8.69 | 3.12 | 5.5 | 0.001 | 10.6 | 0 |
| aegisx | 80788.3 | 1.58 | 2.09 | 12.34 | 6.33 | 5.94 | 0.002 | 34.8 | 0 |
| lab | 113509.7 | 1.11 | 1.43 | 8.81 | 3.47 | 5.19 | 0.001 | 13.8 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 104022.5 | 4.87 | 5.62 | 9.61 | 3.71 | 5.9 | 0.0 | 14.3 | 0 |
| aegisx | 74544.7 | 6.5 | 9.58 | 13.41 | 6.96 | 6.84 | 0.002 | 64.0 | 0 |
| lab | 94422.4 | 5.45 | 5.95 | 10.59 | 4.43 | 6.07 | 0.001 | 34.1 | 0 |
