# aws-r2-h2-static-2w

mode=static protocol=h2 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

h2load with 10 streams per connection; p50 column holds the mean request time and p99 the maximum, h2load reports no percentiles.

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 236161.9 | 5.29 | 20.9 | 8.45 | 5.8 | 2.65 | 0.001 | 25.6 | 0 |
| aegisx | 228940.0 | 5.48 | 12.0 | 8.54 | 7.84 | 0.71 | 0.001 | 54.8 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 230837.5 | 21.29 | 74.67 | 8.63 | 5.95 | 2.68 | 0.001 | 38.3 | 0 |
| aegisx | 197722.0 | 25.09 | 43.98 | 9.81 | 8.84 | 0.97 | 0.001 | 104.0 | 0 |
