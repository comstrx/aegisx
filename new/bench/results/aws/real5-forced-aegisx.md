# real5-forced-aegisx

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 64 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| aegisx | 30035.3 | 2.46 | 13.0 | 13.68 | 8.17 | 5.48 | 0.794 | 64.3 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| aegisx | 28330.8 | 11.34 | 50.43 | 15.13 | 8.7 | 6.43 | 0.8 | 78.0 | 0 |
