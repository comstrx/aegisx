# aws-r0-2w

workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 149490.1 | 0.621 | 1.67 | 13.34 | 4.41 | 8.92 | 0.003 | 17.5 | 0 |
| aegisx | 115935.4 | 0.88 | 2.2 | 17.23 | 8.16 | 8.96 | 0.001 | 37.3 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 140021.9 | 2.7 | 6.12 | 14.25 | 4.61 | 9.64 | 0.003 | 20.8 | 0 |
| aegisx | 108923.8 | 3.86 | 6.9 | 18.34 | 8.61 | 9.71 | 0.001 | 65.5 | 0 |
