# r2a-2w

mode=proxy protocol=h1 workers=2 threads=2 seconds=10 trials=4 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 151115.8 | 0.627 | 1.64 | 13.19 | 4.35 | 8.83 | 0.004 | 17.6 | 0 |
| aegisx | 127915.6 | 0.771 | 1.855 | 15.61 | 7.2 | 8.4 | 0.002 | 28.7 | 0 |
| v11-pin | 129033.8 | 0.786 | 1.975 | 15.42 | 7.18 | 8.3 | 0.002 | 30.3 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 139754.8 | 2.715 | 6.165 | 14.27 | 4.7 | 9.57 | 0.003 | 20.8 | 0 |
| aegisx | 112880.0 | 3.775 | 6.625 | 17.6 | 8.32 | 9.31 | 0.002 | 51.6 | 0 |
| v11-pin | 113732.8 | 3.66 | 6.86 | 17.56 | 8.24 | 9.31 | 0.002 | 54.7 | 0 |
