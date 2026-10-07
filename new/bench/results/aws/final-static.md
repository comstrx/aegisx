# final-static

mode=static protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 267733.4 | 0.395 | 0.741 | 7.45 | 3.09 | 4.37 | 0.001 | 16.4 | 0 |
| nginx-main | 267267.3 | 0.39 | 0.755 | 7.46 | 3.06 | 4.41 | 0.001 | 17.6 | 0 |
| aegisx | 273485.2 | 0.38 | 0.807 | 7.29 | 3.29 | 4.0 | 0.001 | 34.4 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 253197.8 | 1.52 | 3.53 | 7.88 | 3.07 | 4.83 | 0.001 | 16.6 | 0 |
| nginx-main | 248029.9 | 1.65 | 3.5 | 8.04 | 3.12 | 4.92 | 0.001 | 17.8 | 0 |
| aegisx | 252267.5 | 1.57 | 3.53 | 7.9 | 3.45 | 4.46 | 0.001 | 38.3 | 0 |
