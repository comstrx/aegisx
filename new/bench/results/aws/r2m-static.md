# r2m-static

mode=static protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 270690.3 | 0.385 | 0.774 | 7.37 | 3.01 | 4.36 | 0.001 | 16.5 | 0 |
| aegisx | 278131.8 | 0.352 | 0.828 | 7.17 | 3.13 | 4.04 | 0.001 | 36.5 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 255020.2 | 1.44 | 3.8 | 7.82 | 3.08 | 4.79 | 0.001 | 16.7 | 0 |
| aegisx | 261038.4 | 1.45 | 3.45 | 7.65 | 3.29 | 4.36 | 0.001 | 40.9 | 0 |
