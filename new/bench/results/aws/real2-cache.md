# real2-cache

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 64 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 55592.9 | 0.188 | 10.86 | 14.75 | 6.04 | 8.66 | 0.49 | 19.1 | 0 |
| nginx-main | 60577.8 | 0.175 | 10.59 | 13.85 | 5.9 | 7.91 | 0.47 | 20.4 | 0 |
| aegisx | 20959.4 | 3.66 | 27.69 | 14.42 | 8.51 | 5.87 | 0.886 | 62.6 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 45911.0 | 6.52 | 106.14 | 16.72 | 6.56 | 10.16 | 0.513 | 21.0 | 0 |
| nginx-main | 59699.9 | 4.85 | 37.95 | 14.29 | 6.01 | 8.3 | 0.463 | 22.8 | 0 |
| aegisx | 15896.5 | 19.51 | 180.43 | 16.52 | 9.78 | 6.72 | 0.933 | 75.0 | 0 |
