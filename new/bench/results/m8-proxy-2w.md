# m8-proxy-2w

workers=2 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 29696.0 | 4.14 | 7.32 | 66.41 | 18.65 | 48.66 | 0.004 | 17.5 | 0 |
| aegisx | 28302.6 | 4.36 | 7.17 | 69.69 | 21.17 | 48.52 | 0.005 | 30.4 | 0 |
