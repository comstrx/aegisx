# r2m-h1tls

mode=proxy protocol=h1tls workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 110296.5 | 0.86 | 2.11 | 18.07 | 8.47 | 9.59 | 0.004 | 23.7 | 0 |
| aegisx | 114630.0 | 0.89 | 1.89 | 17.37 | 8.67 | 8.81 | 0.002 | 43.7 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 99431.4 | 3.77 | 8.12 | 19.96 | 9.46 | 10.5 | 0.004 | 41.6 | 0 |
| aegisx | 102475.1 | 3.85 | 8.65 | 19.17 | 9.72 | 9.54 | 0.003 | 75.4 | 0 |
