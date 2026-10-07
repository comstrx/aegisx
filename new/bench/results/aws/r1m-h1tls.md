# r1m-h1tls

mode=proxy protocol=h1tls workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 111609.7 | 0.804 | 2.16 | 17.86 | 8.37 | 9.49 | 0.004 | 23.7 | 0 |
| aegisx | 113887.4 | 0.88 | 2.09 | 17.49 | 8.71 | 8.9 | 0.002 | 42.8 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 103576.0 | 3.67 | 7.99 | 19.16 | 9.05 | 10.14 | 0.004 | 41.8 | 0 |
| aegisx | 103916.2 | 3.9 | 7.7 | 19.01 | 9.69 | 9.4 | 0.003 | 71.3 | 0 |
