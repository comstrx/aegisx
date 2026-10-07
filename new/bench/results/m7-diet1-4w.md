# m7-diet1-4w

workers=4 threads=2 seconds=6 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 65413.4 | 1.79 | 11.22 | 57.83 | 19.32 | 39.01 | 0.021 | 31.3 | 0 |
| aegisx | 61617.5 | 1.77 | 17.19 | 55.86 | 20.57 | 35.09 | 0.04 | 33.9 | 0 |
