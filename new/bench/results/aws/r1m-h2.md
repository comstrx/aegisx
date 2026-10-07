# r1m-h2

mode=proxy protocol=h2 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

h2load with 10 streams per connection; p50 column holds the mean request time and p99 the maximum, h2load reports no percentiles.

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 110615.6 | 11.49 | 1070.0 | 18.04 | 9.62 | 8.45 | 0.003 | 35.4 | 0 |
| aegisx | 108416.4 | 11.75 | 31.68 | 18.4 | 12.69 | 5.71 | 0.002 | 100.9 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 91923.7 | 54.6 | 214.39 | 21.74 | 11.5 | 10.33 | 0.004 | 78.3 | 0 |
| aegisx | 84865.2 | 59.31 | 161.26 | 23.46 | 15.66 | 7.78 | 0.004 | 247.4 | 449 |
