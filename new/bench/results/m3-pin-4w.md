# m3-pin-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 31101.6 | 2.36 | 42.35 | 55.72 | 20.58 | 35.41 | 0.162 | 31.5 | 0 |
| aegisx | 37534.6 | 2.33 | 31.54 | 59.62 | 28.87 | 33.19 | 0.119 | 31.9 | 0 |
