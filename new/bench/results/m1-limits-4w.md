# m1-limits-4w

workers=4 threads=2 seconds=12 trials=4 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 57054.1 | 1.875 | 19.085 | 50.87 | 17.88 | 32.3 | 0.151 | 31.7 | 0 |
| aegisx | 51976.7 | 2.11 | 18.125 | 55.8 | 24.2 | 31.21 | 0.16 | 23.3 | 0 |
