# m6-batchE4-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 69126.2 | 1.77 | 12.63 | 55.4 | 18.57 | 36.83 | 0.015 | 30.8 | 0 |
| nginx-log | 60844.6 | 1.85 | 18.58 | 54.62 | 19.4 | 35.36 | 0.075 | 31.0 | 0 |
| aegisx | 63536.6 | 1.84 | 12.05 | 60.01 | 22.6 | 37.41 | 0.013 | 31.9 | 0 |
| access | 58309.7 | 1.97 | 12.33 | 64.34 | 26.1 | 38.24 | 0.046 | 35.8 | 0 |
