# aws-r5-h2-2w

mode=proxy protocol=h2 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

h2load with 10 streams per connection; p50 column holds the mean request time and p99 the maximum, h2load reports no percentiles.

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 106512.9 | 11.95 | 33.8 | 18.76 | 9.9 | 8.82 | 0.003 | 37.8 | 0 |
| aegisx | 97477.2 | 13.04 | 40.81 | 20.34 | 13.77 | 6.56 | 0.003 | 102.8 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 81097.3 | 61.86 | 208.41 | 24.7 | 11.9 | 12.9 | 0.004 | 80.2 | 0 |
| aegisx | 73109.2 | 68.75 | 155.86 | 27.25 | 18.33 | 8.92 | 0.004 | 223.0 | 0 |
