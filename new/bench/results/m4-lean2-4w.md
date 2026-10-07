# m4-lean2-4w

workers=4 threads=2 seconds=8 trials=4 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 66557.5 | 1.75 | 14.155 | 57.29 | 19.96 | 37.33 | 0.014 | 31.8 | 0 |
| aegisx | 65998.5 | 1.72 | 11.49 | 57.64 | 21.9 | 35.74 | 0.017 | 31.6 | 0 |
