# fin2-h3

mode=proxy protocol=h3 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

h3load (bench/h3load: quinn + h3) with 10 streams per connection, closed loop

## 16 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx-main | 128254.2 | 1.123 | 3.046 | 15.2 | 7.8 | 7.45 | 0.007 | 28.3 | 560 |
| caddy | 12745.3 | 12.422 | 24.098 | 156.68 | 126.95 | 29.28 | 0.097 | 66.0 | 0 |
| aegisx | 0.0 | 0.0 | 0.0 | 19950000.0 | 8810000.0 | 11140000.0 | 10870.0 | 49.8 | 1399778 |

## 64 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx-main | 121685.2 | 4.215 | 11.776 | 16.69 | 8.8 | 7.84 | 0.004 | 48.6 | 1921 |
| caddy | 11167.3 | 60.715 | 111.548 | 191.59 | 154.27 | 37.61 | 0.095 | 120.6 | 0 |
| aegisx | 0.0 | 0.0 | 0.0 | 20500000.0 | 9580000.0 | 10920000.0 | 10735.0 | 66.2 | 1337962 |
