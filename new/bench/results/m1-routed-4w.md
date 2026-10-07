# m1-routed-4w

workers=4 threads=2 seconds=12 trials=4 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 64905.8 | 1.645 | 15.435 | 50.51 | 17.99 | 32.34 | 0.085 | 31.7 | 0 |
| aegisx | 69707.8 | 1.655 | 12.2 | 54.92 | 23.65 | 30.98 | 0.017 | 23.2 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 66040.8 | 3.62 | 18.76 | 56.04 | 19.18 | 37.1 | 0.027 | 32.5 | 0 |
| aegisx | 71573.7 | 3.255 | 15.535 | 53.5 | 23.97 | 29.94 | 0.017 | 30.0 | 0 |
