# m4-lean3-8w

workers=8 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 74975.7 | 1.16 | 14.36 | 49.73 | 19.77 | 30.15 | 0.28 | 59.6 | 0 |
| aegisx | 53605.6 | 1.46 | 11.44 | 62.3 | 28.88 | 33.48 | 0.769 | 40.0 | 0 |
