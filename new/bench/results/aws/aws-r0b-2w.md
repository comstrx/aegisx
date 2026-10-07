# aws-r0b-2w

workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 155127.8 | 0.534 | 1.75 | 12.86 | 4.14 | 8.78 | 0.003 | 17.7 | 0 |
| aegisx | 108839.7 | 1.19 | 2.94 | 18.13 | 8.94 | 9.21 | 0.002 | 35.8 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 134336.6 | 3.03 | 5.8 | 14.85 | 4.84 | 10.06 | 0.003 | 20.8 | 0 |
| aegisx | 107098.3 | 3.41 | 10.38 | 18.3 | 8.68 | 9.65 | 0.002 | 65.7 | 0 |
