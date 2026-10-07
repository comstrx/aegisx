# real3-forced

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 64 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 24714.4 | 3.03 | 15.82 | 18.47 | 7.81 | 10.66 | 0.831 | 19.2 | 0 |
| nginx-main | 27760.2 | 2.68 | 13.58 | 16.64 | 7.37 | 9.11 | 0.81 | 20.5 | 0 |
| aegisx | 23533.1 | 3.22 | 17.08 | 15.68 | 9.23 | 6.45 | 0.857 | 59.8 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 20235.5 | 16.61 | 109.68 | 20.29 | 8.05 | 12.25 | 0.852 | 21.1 | 0 |
| nginx-main | 26551.2 | 12.86 | 67.05 | 16.87 | 7.64 | 9.39 | 0.814 | 22.5 | 0 |
| aegisx | 21966.9 | 13.53 | 76.72 | 16.18 | 9.78 | 6.43 | 0.886 | 74.0 | 0 |
