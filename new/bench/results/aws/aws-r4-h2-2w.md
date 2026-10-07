# aws-r4-h2-2w

mode=proxy protocol=h2 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

h2load with 10 streams per connection; p50 column holds the mean request time and p99 the maximum, h2load reports no percentiles.

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 106909.0 | 11.88 | 39.98 | 18.69 | 9.85 | 8.84 | 0.003 | 35.9 | 0 |
| aegisx | 103862.4 | 12.24 | 34.25 | 19.21 | 13.12 | 6.16 | 0.002 | 92.7 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 81197.1 | 61.8 | 216.63 | 24.39 | 11.45 | 12.96 | 0.005 | 79.8 | 0 |
| aegisx | 74640.6 | 67.36 | 155.11 | 26.67 | 17.85 | 8.83 | 0.004 | 219.8 | 0 |
