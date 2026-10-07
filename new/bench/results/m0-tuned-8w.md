# m0-tuned-8w

workers=8 threads=2 seconds=12 trials=4 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|
| nginx | 80303.7 | 1.115 | 13.48 | 49.43 | 0.273 | 59.5 | 0 |
| aegisx | 55412.6 | 1.405 | 14.91 | 62.9 | 0.627 | 30.1 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|
| nginx | 79375.5 | 2.24 | 17.955 | 49.54 | 0.185 | 60.5 | 0 |
| aegisx | 55612.8 | 2.65 | 12.79 | 62.68 | 0.639 | 37.5 | 0 |
