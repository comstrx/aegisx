# real2-blob

mode=proxy protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 32 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 1471.7 | 17.02 | 218.3 | 1240.58 | 148.08 | 1090.55 | 5.731 | 16.9 | 0 |
| nginx-main | 1472.0 | 18.0 | 220.02 | 1220.91 | 145.9 | 1074.26 | 6.322 | 18.0 | 0 |
| caddy | 1471.9 | 18.7 | 220.93 | 1148.8 | 283.87 | 862.02 | 8.83 | 56.2 | 0 |
| aegisx | 1471.2 | 16.44 | 215.33 | 1167.49 | 226.95 | 943.41 | 8.452 | 45.5 | 0 |
