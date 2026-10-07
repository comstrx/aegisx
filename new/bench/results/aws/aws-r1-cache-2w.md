# aws-r1-cache-2w

workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 197305.3 | 0.555 | 0.94 | 10.11 | 3.86 | 6.18 | 0.001 | 18.5 | 0 |
| aegisx | 283388.7 | 0.302 | 0.9 | 7.05 | 3.27 | 3.78 | 0.001 | 31.5 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 189226.7 | 2.25 | 4.03 | 10.54 | 4.0 | 6.58 | 0.001 | 21.7 | 0 |
| aegisx | 255411.7 | 1.62 | 3.51 | 7.78 | 3.55 | 4.22 | 0.001 | 41.9 | 0 |
