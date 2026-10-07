# fin2-static

mode=static protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 264877.2 | 0.391 | 0.785 | 7.53 | 3.07 | 4.41 | 0.001 | 16.6 | 0 |
| aegisx | 270779.6 | 0.348 | 0.835 | 7.37 | 3.34 | 4.03 | 0.001 | 32.4 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 249160.6 | 1.45 | 3.85 | 8.01 | 3.09 | 4.93 | 0.001 | 16.8 | 0 |
| aegisx | 246837.9 | 1.59 | 3.49 | 8.09 | 3.5 | 4.58 | 0.001 | 41.7 | 0 |
