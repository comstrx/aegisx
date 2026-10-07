# m8-proxy2-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 72083.8 | 1.63 | 12.56 | 56.45 | 19.37 | 37.09 | 0.014 | 31.3 | 0 |
| aegisx | 54721.9 | 1.88 | 631.42 | 58.35 | 22.48 | 35.21 | 0.027 | 34.8 | 0 |
