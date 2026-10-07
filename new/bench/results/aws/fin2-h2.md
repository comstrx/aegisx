# fin2-h2

mode=proxy protocol=h2 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

h2load with 10 streams per connection; p50 column holds the mean request time and p99 the maximum, h2load reports no percentiles.

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 105555.4 | 12.03 | 46.88 | 18.84 | 9.7 | 9.13 | 0.003 | 34.9 | 0 |
| aegisx | 106739.7 | 11.93 | 32.76 | 18.68 | 12.93 | 5.68 | 0.002 | 101.2 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 85658.1 | 58.56 | 230.24 | 23.34 | 12.13 | 11.27 | 0.005 | 83.8 | 0 |
| aegisx | 78715.8 | 63.89 | 158.66 | 25.28 | 16.83 | 8.45 | 0.004 | 255.8 | 469 |
