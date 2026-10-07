# m6-batchE3-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 68693.6 | 1.72 | 13.26 | 56.07 | 19.7 | 36.37 | 0.019 | 30.7 | 0 |
| nginx-log | 64714.6 | 1.79 | 12.74 | 57.9 | 20.19 | 37.42 | 0.032 | 31.0 | 0 |
| aegisx | 62712.1 | 1.88 | 12.16 | 58.73 | 22.34 | 36.3 | 0.028 | 29.6 | 0 |
| access | 57892.3 | 2.06 | 15.42 | 65.06 | 25.96 | 38.79 | 0.02 | 36.5 | 0 |
| compress | 64472.2 | 1.85 | 10.83 | 59.19 | 22.09 | 37.81 | 0.014 | 29.8 | 0 |
