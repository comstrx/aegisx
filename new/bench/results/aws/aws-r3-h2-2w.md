# aws-r3-h2-2w

mode=proxy protocol=h2 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

h2load with 10 streams per connection; p50 column holds the mean request time and p99 the maximum, h2load reports no percentiles.

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 106630.6 | 11.94 | 47.3 | 18.72 | 9.82 | 8.9 | 0.003 | 35.4 | 0 |
| aegisx | 102320.5 | 12.43 | 32.33 | 19.5 | 13.23 | 6.26 | 0.002 | 98.6 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 83291.6 | 60.26 | 194.93 | 24.06 | 12.13 | 11.8 | 0.005 | 78.1 | 0 |
| aegisx | 73196.8 | 68.74 | 154.71 | 27.17 | 18.39 | 8.78 | 0.004 | 222.9 | 0 |
