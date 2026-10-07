# real5-forced-nginx-main

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 64 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx-main | 30352.8 | 2.41 | 12.64 | 15.11 | 6.66 | 8.45 | 0.784 | 20.7 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx-main | 28041.2 | 11.84 | 53.52 | 16.53 | 7.16 | 9.37 | 0.799 | 22.6 | 0 |
