# aws-r3-2w

mode=proxy protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 151306.7 | 0.637 | 1.68 | 13.16 | 4.37 | 8.81 | 0.003 | 18.4 | 0 |
| aegisx | 111285.6 | 1.05 | 2.36 | 17.97 | 9.0 | 8.97 | 0.002 | 34.2 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 138790.8 | 2.65 | 6.62 | 14.37 | 4.7 | 9.67 | 0.003 | 21.6 | 0 |
| aegisx | 102947.4 | 3.8 | 9.76 | 19.4 | 9.45 | 9.96 | 0.003 | 64.7 | 0 |
