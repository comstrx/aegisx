# m6-batchE5-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 66677.8 | 1.79 | 15.23 | 54.82 | 18.39 | 35.84 | 0.033 | 30.7 | 0 |
| nginx-log | 64758.2 | 1.84 | 13.58 | 58.41 | 20.5 | 37.74 | 0.018 | 31.1 | 0 |
| aegisx | 55209.8 | 1.83 | 20.43 | 54.64 | 21.77 | 32.23 | 0.135 | 29.8 | 0 |
| access | 51010.8 | 2.02 | 17.0 | 60.07 | 24.15 | 35.92 | 0.09 | 40.8 | 0 |
