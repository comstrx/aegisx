# r3v-1w

mode=proxy protocol=h1 workers=1 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 111124.9 | 1.14 | 1.49 | 8.99 | 3.28 | 5.84 | 0.001 | 10.5 | 0 |
| aegisx | 89788.7 | 1.41 | 2.2 | 11.11 | 5.51 | 5.58 | 0.002 | 35.8 | 0 |
| v13 | 94740.0 | 1.34 | 1.69 | 10.55 | 5.3 | 5.25 | 0.001 | 32.9 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 102140.4 | 4.98 | 5.64 | 9.78 | 3.83 | 6.19 | 0.001 | 13.3 | 0 |
| aegisx | 79391.9 | 6.37 | 7.41 | 12.58 | 6.03 | 6.42 | 0.002 | 59.2 | 0 |
| v13 | 84263.7 | 6.11 | 6.82 | 11.87 | 6.16 | 5.81 | 0.001 | 56.3 | 0 |
