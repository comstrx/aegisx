# m3-telemetry-4w

workers=4 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 32090.4 | 2.69 | 32.05 | 52.43 | 18.78 | 33.33 | 0.318 | 31.6 | 0 |
| aegisx | 19281.0 | 5.13 | 54.61 | 64.42 | 31.66 | 32.76 | 0.454 | 29.6 | 0 |
