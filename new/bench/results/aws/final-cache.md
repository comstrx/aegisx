# final-cache

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 195618.2 | 0.557 | 1.08 | 10.2 | 3.79 | 6.41 | 0.001 | 17.6 | 0 |
| nginx-main | 200153.6 | 0.482 | 1.11 | 9.97 | 3.6 | 6.38 | 0.001 | 18.9 | 0 |
| aegisx | 284591.4 | 0.337 | 0.794 | 7.01 | 3.04 | 3.96 | 0.001 | 34.4 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 187637.5 | 2.13 | 4.28 | 10.63 | 3.92 | 6.77 | 0.001 | 21.2 | 0 |
| nginx-main | 186254.6 | 2.18 | 4.33 | 10.58 | 3.83 | 6.75 | 0.001 | 19.7 | 0 |
| aegisx | 256029.1 | 1.48 | 3.53 | 7.79 | 3.42 | 4.37 | 0.001 | 52.4 | 0 |
