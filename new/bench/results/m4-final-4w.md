# m4-final-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 45667.9 | 2.0 | 35.16 | 72.52 | 23.68 | 48.55 | 0.04 | 31.5 | 0 |
| aegisx | 64822.1 | 1.77 | 11.16 | 57.73 | 21.5 | 36.23 | 0.017 | 31.7 | 0 |
