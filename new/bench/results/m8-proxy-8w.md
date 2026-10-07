# m8-proxy-8w

workers=8 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 46254.5 | 1.73 | 16.66 | 62.11 | 23.57 | 38.53 | 0.37 | 59.1 | 0 |
| aegisx | 62728.8 | 1.41 | 16.9 | 59.35 | 27.49 | 33.36 | 0.285 | 44.4 | 0 |
