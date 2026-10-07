# m1-routed-8w

workers=8 threads=2 seconds=12 trials=4 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 61167.1 | 1.41 | 15.3 | 51.41 | 20.2 | 31.22 | 0.329 | 59.6 | 0 |
| aegisx | 53921.2 | 1.505 | 14.535 | 65.5 | 33.0 | 32.63 | 0.544 | 30.3 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 53300.5 | 3.17 | 25.77 | 57.98 | 22.82 | 35.16 | 0.27 | 60.7 | 0 |
| aegisx | 48403.7 | 3.205 | 21.79 | 75.08 | 37.23 | 37.62 | 0.437 | 37.4 | 0 |
