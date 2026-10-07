# m4-buffers-8w

workers=8 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 82007.0 | 1.06 | 16.51 | 48.79 | 18.77 | 29.31 | 0.269 | 59.4 | 0 |
| nopin | 79060.9 | 1.12 | 14.66 | 49.77 | 20.86 | 28.91 | 0.265 | 35.9 | 0 |
| small | 66849.7 | 1.3 | 14.45 | 48.35 | 20.93 | 28.31 | 0.281 | 35.4 | 0 |
