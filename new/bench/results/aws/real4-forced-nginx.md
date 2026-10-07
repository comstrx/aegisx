# real4-forced-nginx

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 64 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 29082.6 | 2.54 | 13.04 | 17.91 | 8.09 | 10.04 | 0.782 | 19.6 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 27800.9 | 11.78 | 52.77 | 18.57 | 8.05 | 10.53 | 0.785 | 21.3 | 0 |
