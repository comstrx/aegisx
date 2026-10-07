# m4-base-2w

workers=2 threads=2 seconds=8 trials=3 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 128 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 22407.2 | 4.86 | 30.19 | 82.88 | 23.56 | 59.32 | 0.005 | 17.6 | 0 |
| aegisx | 25453.4 | 4.49 | 15.35 | 76.44 | 28.67 | 47.78 | 0.003 | 27.7 | 0 |
