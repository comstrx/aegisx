# m8-static2-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 93574.9 | 0.788 | 15.78 | 26.76 | 12.37 | 14.38 | 0.05 | 30.1 | 0 |
| aegisx | 83133.9 | 0.826 | 16.54 | 27.09 | 13.07 | 14.32 | 0.062 | 33.7 | 0 |
