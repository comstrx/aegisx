# real4-cache-nginx-main

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 64 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx-main | 21035.7 | 3.57 | 27.95 | 18.82 | 8.13 | 10.69 | 0.863 | 20.8 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx-main | 20010.9 | 16.21 | 150.7 | 21.62 | 8.89 | 12.73 | 0.851 | 23.0 | 0 |
