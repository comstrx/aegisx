# real5-cache-aegisx

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 64 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| aegisx | 24751.9 | 3.03 | 20.69 | 13.96 | 8.31 | 5.74 | 0.853 | 68.1 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| aegisx | 23016.3 | 13.29 | 86.9 | 16.15 | 9.72 | 6.42 | 0.85 | 82.5 | 0 |
