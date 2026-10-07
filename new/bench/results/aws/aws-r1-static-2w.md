# aws-r1-static-2w

workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 274814.6 | 0.397 | 0.729 | 7.26 | 3.15 | 4.16 | 0.001 | 17.3 | 0 |
| aegisx | 270835.7 | 0.369 | 0.77 | 7.37 | 3.46 | 3.9 | 0.001 | 29.3 | 0 |

## 512 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 258010.7 | 1.54 | 3.31 | 7.74 | 3.15 | 4.59 | 0.001 | 17.5 | 0 |
| aegisx | 247777.6 | 1.59 | 3.5 | 8.06 | 3.77 | 4.29 | 0.001 | 40.1 | 0 |
