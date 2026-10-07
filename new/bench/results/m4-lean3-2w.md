# m4-lean3-2w

workers=2 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 25263.2 | 4.46 | 19.77 | 75.87 | 21.56 | 54.31 | 0.005 | 17.9 | 0 |
| aegisx | 26737.2 | 4.35 | 21.1 | 72.99 | 23.35 | 49.03 | 0.005 | 27.2 | 0 |
