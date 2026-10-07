# aws-r2-h1tls-2w

mode=proxy protocol=h1tls workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors | full hs/s | resumed hs/s | server µs/full hs | server µs/resumed hs |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 106912.1 | 0.94 | 2.0 | 18.63 | 8.89 | 9.74 | 0.004 | 25.2 | 0 | 786.5 | 907.2 | 327.5 | 327.9 |
| aegisx | 102700.4 | 1.19 | 2.81 | 19.4 | 10.08 | 9.31 | 0.002 | 41.9 | 0 | 869.5 | 987.0 | 155.8 | 155.4 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors | full hs/s | resumed hs/s | server µs/full hs | server µs/resumed hs |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 99588.7 | 3.67 | 8.17 | 19.93 | 9.57 | 10.28 | 0.004 | 42.4 | 0 | 927.5 | 904.8 | 333.6 | 328.5 |
| aegisx | 93929.1 | 3.92 | 9.24 | 21.05 | 10.9 | 10.13 | 0.004 | 70.7 | 0 | 822.0 | 988.2 | 161.2 | 161.9 |
