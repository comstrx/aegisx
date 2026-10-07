# aws-r4-h1tls-2w

mode=proxy protocol=h1tls workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 112632.8 | 0.785 | 2.06 | 17.7 | 8.49 | 9.36 | 0.003 | 25.0 | 0 |
| aegisx | 98026.2 | 1.15 | 2.49 | 20.34 | 10.74 | 9.52 | 0.002 | 41.8 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 98147.8 | 4.07 | 7.8 | 20.21 | 9.55 | 10.45 | 0.004 | 43.1 | 0 |
| aegisx | 93180.9 | 4.18 | 11.64 | 21.19 | 11.05 | 10.25 | 0.004 | 73.7 | 0 |
