# m0-engines-8w

workers=8 threads=2 seconds=12 trials=4 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|
| direct | 184890.8 | 0.398 | 1.58 | 8.19 | 0.056 | 3.5 | 0 |
| nginx | 66531.3 | 1.315 | 15.17 | 54.73 | 0.276 | 59.4 | 0 |
| pingora | 37295.8 | 3.05 | 13.42 | 129.6 | 0.567 | 24.3 | 0 |
| aegisx | 52163.5 | 1.54 | 14.39 | 66.21 | 0.628 | 27.8 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|
| direct | 177965.6 | 0.762 | 2.885 | 8.52 | 0.047 | 6.7 | 0 |
| nginx | 65496.1 | 2.66 | 21.445 | 52.43 | 0.212 | 60.4 | 0 |
| pingora | 31339.1 | 7.35 | 25.02 | 134.65 | 0.656 | 42.9 | 0 |
| aegisx | 49128.2 | 3.13 | 26.33 | 69.62 | 0.527 | 36.3 | 0 |
