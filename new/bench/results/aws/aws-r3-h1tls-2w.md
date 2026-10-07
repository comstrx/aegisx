# aws-r3-h1tls-2w

mode=proxy protocol=h1tls workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 109635.8 | 0.827 | 2.36 | 18.18 | 8.7 | 9.56 | 0.004 | 25.6 | 0 |
| aegisx | 98485.8 | 1.28 | 3.23 | 20.23 | 10.66 | 9.46 | 0.002 | 41.3 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 97670.0 | 3.98 | 8.65 | 19.93 | 9.55 | 10.39 | 0.004 | 42.3 | 0 |
| aegisx | 95449.3 | 3.94 | 11.06 | 20.71 | 10.71 | 10.09 | 0.004 | 72.8 | 0 |
