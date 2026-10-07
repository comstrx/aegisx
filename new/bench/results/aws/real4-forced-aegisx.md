# real4-forced-aegisx

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 64 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| aegisx | 29292.7 | 2.53 | 13.04 | 15.33 | 8.82 | 6.55 | 0.793 | 64.1 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| aegisx | 27965.3 | 11.99 | 56.5 | 15.9 | 9.43 | 6.47 | 0.79 | 76.4 | 0 |
