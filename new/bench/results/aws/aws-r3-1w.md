# aws-r3-1w

mode=proxy protocol=h1 workers=1 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 113672.3 | 1.12 | 1.35 | 8.78 | 3.62 | 5.16 | 0.0 | 11.3 | 0 |
| aegisx | 80576.9 | 1.53 | 1.86 | 12.39 | 6.65 | 6.19 | 0.002 | 29.5 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 101082.0 | 5.01 | 5.83 | 9.89 | 4.06 | 5.86 | 0.0 | 14.6 | 0 |
| aegisx | 69079.8 | 6.83 | 10.88 | 14.48 | 6.74 | 7.32 | 0.004 | 58.7 | 0 |
