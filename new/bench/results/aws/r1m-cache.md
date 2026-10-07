# r1m-cache

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 199790.0 | 0.563 | 1.08 | 10.0 | 3.75 | 6.25 | 0.001 | 17.6 | 0 |
| aegisx | 288614.9 | 0.322 | 0.816 | 6.91 | 3.01 | 3.9 | 0.001 | 33.1 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 188163.6 | 2.36 | 3.89 | 10.61 | 3.95 | 6.67 | 0.001 | 21.3 | 0 |
| aegisx | 262753.7 | 1.54 | 3.46 | 7.59 | 3.16 | 4.43 | 0.001 | 37.2 | 0 |
