# r3-cache

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 194971.7 | 0.537 | 1.02 | 10.24 | 3.9 | 6.32 | 0.001 | 17.7 | 0 |
| aegisx | 284523.5 | 0.36 | 0.735 | 7.01 | 3.04 | 3.97 | 0.001 | 34.8 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 186823.7 | 2.23 | 3.99 | 10.68 | 3.97 | 6.69 | 0.001 | 20.6 | 0 |
| aegisx | 266028.3 | 1.5 | 3.38 | 7.5 | 3.14 | 4.36 | 0.001 | 42.1 | 0 |
