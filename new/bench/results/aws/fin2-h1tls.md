# fin2-h1tls

mode=proxy protocol=h1tls workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 109305.2 | 0.93 | 2.16 | 18.04 | 8.48 | 9.56 | 0.004 | 24.0 | 0 |
| aegisx | 112542.7 | 1.0 | 1.8 | 17.69 | 8.87 | 8.84 | 0.002 | 44.1 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 99707.9 | 3.74 | 8.34 | 19.9 | 9.27 | 10.53 | 0.004 | 41.4 | 0 |
| aegisx | 99631.5 | 3.73 | 8.97 | 19.83 | 10.21 | 9.55 | 0.003 | 73.0 | 0 |
