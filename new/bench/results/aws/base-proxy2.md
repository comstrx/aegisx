# base-proxy2

mode=proxy protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 153453.5 | 0.616 | 1.62 | 12.93 | 4.19 | 8.7 | 0.003 | 17.6 | 0 |
| aegisx | 114958.1 | 1.13 | 2.72 | 17.37 | 8.47 | 8.9 | 0.002 | 38.0 | 0 |
| lab | 145435.4 | 0.703 | 1.67 | 13.7 | 5.22 | 8.49 | 0.002 | 15.4 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 138050.5 | 2.62 | 6.58 | 14.45 | 4.77 | 9.68 | 0.003 | 20.9 | 0 |
| aegisx | 106292.2 | 3.78 | 8.83 | 18.78 | 9.02 | 9.67 | 0.002 | 67.3 | 0 |
| lab | 127198.7 | 3.08 | 6.74 | 15.64 | 6.1 | 9.52 | 0.002 | 35.6 | 0 |
