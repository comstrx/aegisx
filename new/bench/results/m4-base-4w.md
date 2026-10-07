# m4-base-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 45565.6 | 2.03 | 22.79 | 60.01 | 20.73 | 40.05 | 0.081 | 31.5 | 0 |
| aegisx | 37876.2 | 2.33 | 30.49 | 76.82 | 34.53 | 42.29 | 0.085 | 30.6 | 0 |
