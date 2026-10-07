# real3-cache

mode=cache protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 64 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 20335.4 | 3.76 | 27.98 | 19.71 | 8.1 | 11.61 | 0.86 | 19.3 | 0 |
| nginx-main | 22056.9 | 3.43 | 29.16 | 17.99 | 7.69 | 10.24 | 0.854 | 20.7 | 0 |
| aegisx | 18566.1 | 4.1 | 29.67 | 16.46 | 9.85 | 6.61 | 0.923 | 55.6 | 0 |

## 256 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 17653.4 | 17.88 | 126.82 | 22.61 | 8.94 | 13.52 | 0.885 | 21.5 | 0 |
| nginx-main | 19339.4 | 16.55 | 126.57 | 20.68 | 8.54 | 12.15 | 0.852 | 22.8 | 0 |
| aegisx | 16923.1 | 18.85 | 170.61 | 18.0 | 10.74 | 7.2 | 0.893 | 68.0 | 0 |
