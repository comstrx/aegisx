# real2-upload

mode=proxy protocol=h1 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

## 32 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx | 5294.3 | 5.02 | 15.48 | 167.72 | 23.49 | 144.24 | 1.173 | 17.0 | 0 |
| nginx-main | 5340.8 | 4.99 | 15.43 | 168.04 | 23.23 | 145.47 | 1.168 | 18.3 | 0 |
| caddy | 5281.8 | 5.28 | 13.15 | 311.26 | 141.05 | 171.0 | 0.965 | 55.1 | 0 |
| aegisx | 5287.9 | 4.48 | 11.06 | 165.97 | 39.64 | 127.2 | 1.143 | 29.3 | 0 |
