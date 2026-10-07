# aws-r5-2w

mode=proxy protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 148718.4 | 0.633 | 1.61 | 13.41 | 4.49 | 8.92 | 0.003 | 18.3 | 0 |
| aegisx | 110654.6 | 1.16 | 2.7 | 18.06 | 8.93 | 9.08 | 0.002 | 33.9 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 139337.9 | 2.79 | 6.45 | 14.3 | 4.72 | 9.63 | 0.003 | 21.8 | 0 |
| aegisx | 102545.5 | 3.88 | 9.98 | 19.35 | 9.27 | 9.99 | 0.003 | 66.7 | 0 |
