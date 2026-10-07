# real5-cache-nginx-main

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 64 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx-main | 23368.4 | 3.25 | 28.03 | 16.77 | 7.48 | 9.33 | 0.846 | 20.7 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx-main | 20243.3 | 15.94 | 134.49 | 18.55 | 7.71 | 10.86 | 0.862 | 22.8 | 0 |
