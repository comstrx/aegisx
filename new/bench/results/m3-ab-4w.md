# m3-ab-4w

workers=4 threads=2 seconds=8 trials=4 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 59181.7 | 1.725 | 17.635 | 54.85 | 19.31 | 35.54 | 0.077 | 31.5 | 0 |
| aegisx | 59321.7 | 1.835 | 19.445 | 61.13 | 27.03 | 33.87 | 0.02 | 28.8 | 0 |
| notelemetry | 60906.5 | 1.76 | 16.99 | 59.29 | 25.69 | 34.33 | 0.032 | 29.9 | 0 |
