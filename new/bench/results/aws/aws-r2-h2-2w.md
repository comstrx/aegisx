# aws-r2-h2-2w

mode=proxy protocol=h2 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

h2load with 10 streams per connection; p50 column holds the mean request time and p99 the maximum, h2load reports no percentiles.

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 109036.9 | 11.64 | 33.55 | 18.2 | 9.55 | 8.65 | 0.003 | 36.0 | 0 |
| aegisx | 101320.6 | 12.55 | 41.99 | 19.68 | 13.4 | 6.28 | 0.003 | 79.0 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 83806.3 | 59.89 | 188.77 | 23.88 | 12.33 | 11.81 | 0.004 | 80.7 | 0 |
| aegisx | 44206.4 | 113.39 | 248.0 | 44.11 | 21.29 | 22.71 | 0.019 | 165.5 | 16 |
