# r3-static

mode=static protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 266415.9 | 0.369 | 0.81 | 7.49 | 3.08 | 4.36 | 0.001 | 16.5 | 0 |
| aegisx | 278562.2 | 0.331 | 0.83 | 7.16 | 3.2 | 4.0 | 0.001 | 34.2 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 260767.6 | 1.49 | 3.51 | 7.65 | 2.99 | 4.66 | 0.001 | 16.7 | 0 |
| aegisx | 264520.5 | 1.51 | 3.42 | 7.54 | 3.24 | 4.31 | 0.001 | 38.5 | 0 |
