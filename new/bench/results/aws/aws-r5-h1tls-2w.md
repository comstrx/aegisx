# aws-r5-h1tls-2w

mode=proxy protocol=h1tls workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 107860.1 | 0.92 | 2.04 | 18.46 | 8.89 | 9.57 | 0.004 | 25.2 | 0 |
| aegisx | 100448.7 | 1.3 | 3.09 | 19.85 | 10.53 | 9.41 | 0.002 | 42.1 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 100180.5 | 3.83 | 8.26 | 19.79 | 9.39 | 10.36 | 0.004 | 42.7 | 0 |
| aegisx | 95355.6 | 4.03 | 9.08 | 20.71 | 10.76 | 9.93 | 0.004 | 75.4 | 0 |
