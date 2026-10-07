# aws-base-2w

workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 153131.8 | 0.617 | 1.58 | 13.02 | 4.35 | 8.75 | 0.003 | 17.6 | 0 |
| aegisx | 147073.6 | 0.627 | 1.57 | 13.58 | 5.16 | 8.42 | 0.001 | 34.2 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 139603.9 | 2.82 | 5.98 | 14.29 | 4.63 | 9.67 | 0.003 | 20.7 | 0 |
| aegisx | 135841.8 | 2.83 | 6.03 | 14.69 | 5.51 | 9.19 | 0.001 | 56.9 | 0 |
