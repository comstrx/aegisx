# aws-r5-1w

mode=proxy protocol=h1 workers=1 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 110543.0 | 1.14 | 1.47 | 9.04 | 3.39 | 5.71 | 0.001 | 11.5 | 0 |
| aegisx | 82103.8 | 1.53 | 1.79 | 12.16 | 6.93 | 5.31 | 0.0 | 32.9 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 101196.6 | 5.01 | 5.83 | 9.88 | 4.0 | 5.88 | 0.0 | 14.4 | 0 |
| aegisx | 74489.2 | 6.71 | 8.16 | 13.43 | 7.75 | 5.68 | 0.0 | 60.9 | 0 |
