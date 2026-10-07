# r1m-static

mode=static protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 266269.6 | 0.393 | 0.764 | 7.49 | 3.09 | 4.38 | 0.001 | 16.5 | 0 |
| aegisx | 280528.1 | 0.32 | 0.87 | 7.11 | 3.1 | 4.01 | 0.001 | 31.8 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 254172.3 | 1.5 | 3.59 | 7.8 | 3.04 | 4.81 | 0.001 | 16.7 | 0 |
| aegisx | 263549.5 | 1.54 | 3.34 | 7.57 | 3.22 | 4.35 | 0.001 | 37.6 | 0 |
