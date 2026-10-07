# m4-lean3-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 70270.0 | 1.72 | 9.09 | 55.39 | 18.98 | 36.41 | 0.014 | 31.7 | 0 |
| aegisx | 72041.0 | 1.7 | 10.0 | 54.26 | 19.23 | 35.0 | 0.009 | 31.6 | 0 |
