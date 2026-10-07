# r3-h1tls

mode=proxy protocol=h1tls workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 107598.3 | 0.92 | 2.1 | 18.44 | 8.8 | 9.64 | 0.004 | 24.0 | 0 |
| aegisx | 117574.6 | 0.812 | 2.03 | 16.94 | 8.47 | 8.45 | 0.001 | 44.3 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 97931.6 | 3.84 | 7.6 | 20.25 | 9.63 | 10.61 | 0.004 | 41.9 | 0 |
| aegisx | 102641.5 | 3.78 | 8.95 | 19.24 | 9.92 | 9.33 | 0.003 | 73.3 | 0 |
