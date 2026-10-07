# m1-hotpath-4w-clean

workers=4 threads=2 seconds=12 trials=4 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 56123.1 | 1.85 | 17.305 | 59.74 | 20.66 | 39.08 | 0.059 | 31.8 | 0 |
| aegisx | 48304.7 | 2.005 | 23.635 | 67.18 | 28.55 | 37.79 | 0.083 | 23.2 | 0 |
