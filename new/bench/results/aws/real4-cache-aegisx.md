# real4-cache-aegisx

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 64 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| aegisx | 21245.3 | 3.57 | 26.78 | 16.39 | 9.55 | 6.53 | 0.883 | 66.1 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| aegisx | 15899.4 | 19.82 | 149.52 | 19.22 | 11.93 | 7.22 | 0.934 | 80.3 | 0 |
