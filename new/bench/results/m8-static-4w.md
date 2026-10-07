# m8-static-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 123708.1 | 0.648 | 13.86 | 27.01 | 12.77 | 14.24 | 0.039 | 30.2 | 0 |
| aegisx | 74684.6 | 1.28 | 12.75 | 59.91 | 22.41 | 37.81 | 1.916 | 55.3 | 0 |
