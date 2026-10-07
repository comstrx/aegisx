# aws-r3a-h2-2w

mode=proxy protocol=h2 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

h2load with 10 streams per connection; p50 column holds the mean request time and p99 the maximum, h2load reports no percentiles.

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 108709.2 | 11.7 | 42.11 | 18.37 | 9.65 | 8.69 | 0.003 | 35.2 | 0 |
| aegisx | 99905.0 | 12.72 | 34.79 | 19.96 | 13.65 | 6.31 | 0.002 | 90.7 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 84963.6 | 59.08 | 196.27 | 23.57 | 11.73 | 11.89 | 0.004 | 79.9 | 0 |
| aegisx | 72285.0 | 69.54 | 153.86 | 27.17 | 18.16 | 8.91 | 0.004 | 216.1 | 0 |
