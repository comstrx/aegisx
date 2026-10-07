# final-h1tls

mode=proxy protocol=h1tls workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 105652.9 | 0.97 | 2.06 | 18.86 | 8.93 | 9.93 | 0.004 | 23.8 | 0 |
| nginx-main | 109657.5 | 0.92 | 2.08 | 18.17 | 8.69 | 9.63 | 0.004 | 23.9 | 0 |
| aegisx | 109973.9 | 0.93 | 2.07 | 18.11 | 9.07 | 8.94 | 0.002 | 43.7 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 102196.9 | 3.73 | 8.32 | 19.29 | 9.12 | 10.16 | 0.004 | 40.5 | 0 |
| nginx-main | 99751.9 | 3.96 | 8.79 | 19.75 | 9.28 | 10.48 | 0.004 | 41.3 | 0 |
| aegisx | 101982.3 | 3.91 | 8.8 | 19.37 | 9.81 | 9.56 | 0.003 | 72.4 | 0 |
