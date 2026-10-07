# fin2-cache

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 198140.2 | 0.578 | 0.94 | 10.08 | 3.77 | 6.31 | 0.001 | 17.8 | 0 |
| aegisx | 274036.0 | 0.348 | 0.798 | 7.2 | 3.21 | 4.03 | 0.001 | 31.1 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 185298.2 | 2.21 | 4.15 | 10.77 | 4.04 | 6.73 | 0.001 | 21.8 | 0 |
| aegisx | 259053.1 | 1.51 | 3.4 | 7.71 | 3.29 | 4.42 | 0.001 | 37.1 | 0 |
