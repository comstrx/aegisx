# m4-final-8w

workers=8 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 73052.8 | 1.21 | 16.79 | 52.99 | 20.31 | 32.64 | 0.289 | 59.4 | 0 |
| aegisx | 55570.6 | 1.58 | 16.73 | 60.78 | 24.44 | 36.34 | 0.284 | 38.2 | 0 |
