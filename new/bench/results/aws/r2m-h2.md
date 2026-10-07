# r2m-h2

mode=proxy protocol=h2 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

h2load with 10 streams per connection; p50 column holds the mean request time and p99 the maximum, h2load reports no percentiles.

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 109113.4 | 11.65 | 61.19 | 18.29 | 9.57 | 8.79 | 0.003 | 34.6 | 0 |
| aegisx | 108822.0 | 11.72 | 25.97 | 18.32 | 12.58 | 5.79 | 0.002 | 102.0 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 88801.5 | 56.5 | 200.93 | 22.51 | 11.84 | 10.68 | 0.004 | 79.0 | 0 |
| aegisx | 82773.7 | 60.82 | 147.41 | 24.04 | 15.87 | 8.15 | 0.004 | 249.6 | 158 |
