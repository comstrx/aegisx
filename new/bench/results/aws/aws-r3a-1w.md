# aws-r3a-1w

mode=proxy protocol=h1 workers=1 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 115881.2 | 1.08 | 1.46 | 8.62 | 3.05 | 5.57 | 0.001 | 11.3 | 0 |
| aegisx | 82744.3 | 1.53 | 1.87 | 12.08 | 6.12 | 5.96 | 0.002 | 30.9 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 100860.5 | 5.01 | 5.77 | 9.92 | 3.47 | 6.59 | 0.001 | 15.0 | 0 |
| aegisx | 74606.0 | 6.58 | 9.88 | 13.4 | 7.38 | 6.14 | 0.001 | 58.6 | 0 |
