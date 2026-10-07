# m0-tuned-4w

workers=4 threads=2 seconds=12 trials=4 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|
| nginx | 60485.1 | 1.825 | 16.98 | 58.82 | 0.024 | 31.6 | 0 |
| aegisx | 73293.9 | 1.54 | 12.175 | 51.15 | 0.015 | 22.9 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|
| nginx | 51359.0 | 3.94 | 24.92 | 63.62 | 0.058 | 32.3 | 0 |
| aegisx | 57395.5 | 3.475 | 24.315 | 57.48 | 0.055 | 31.8 | 0 |
