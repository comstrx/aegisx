# real4-forced-nginx-main

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 64 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx-main | 29242.6 | 2.51 | 13.09 | 16.9 | 7.31 | 9.58 | 0.791 | 20.7 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx-main | 28182.8 | 11.68 | 53.97 | 18.96 | 8.26 | 10.7 | 0.785 | 22.6 | 0 |
