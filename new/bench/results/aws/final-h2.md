# final-h2

mode=proxy protocol=h2 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

h2load with 10 streams per connection; p50 column holds the mean request time and p99 the maximum, h2load reports no percentiles.

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 108154.2 | 11.76 | 43.35 | 18.4 | 9.58 | 8.9 | 0.003 | 34.2 | 0 |
| nginx-main | 109861.2 | 11.56 | 60.07 | 18.16 | 9.52 | 8.65 | 0.003 | 34.8 | 0 |
| aegisx | 108455.8 | 11.74 | 29.57 | 18.39 | 12.74 | 5.74 | 0.002 | 98.4 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 87736.2 | 57.24 | 209.59 | 22.83 | 11.83 | 11.02 | 0.004 | 79.9 | 0 |
| nginx-main | 88795.5 | 56.52 | 200.08 | 22.53 | 11.85 | 10.69 | 0.004 | 80.6 | 0 |
| aegisx | 81342.5 | 61.88 | 154.64 | 24.49 | 16.39 | 8.19 | 0.004 | 240.4 | 726 |
