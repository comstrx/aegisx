# m3-nopin-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 58700.5 | 1.69 | 16.36 | 52.56 | 18.34 | 32.65 | 0.112 | 31.5 | 0 |
| aegisx | 48390.7 | 1.92 | 18.53 | 62.15 | 30.1 | 32.05 | 0.111 | 30.2 | 0 |
