# fin3-h3

mode=proxy protocol=h3 workers=2 threads=2 seconds=10 trials=3 host=Linux-7.0.0-1013-aws-x86_64-with-glibc2.39

h3load (bench/h3load: quinn + h3) with 10 streams per connection, closed loop

## 16 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx-main | 129908.5 | 1.192 | 1.844 | 15.34 | 7.88 | 7.42 | 0.004 | 27.3 | 495 |
| caddy | 12673.2 | 12.478 | 24.545 | 157.57 | 128.14 | 29.43 | 0.097 | 68.6 | 0 |
| aegisx | 85233.4 | 1.703 | 3.019 | 23.25 | 15.77 | 7.48 | 0.002 | 57.6 | 0 |

## 64 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | user | sys | ctx sw/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| nginx-main | 124364.1 | 4.213 | 10.821 | 16.37 | 8.67 | 7.65 | 0.004 | 48.4 | 1923 |
| caddy | 11110.2 | 61.716 | 111.377 | 193.77 | 154.74 | 38.43 | 0.095 | 119.0 | 0 |
| aegisx | 82075.1 | 6.826 | 12.555 | 24.91 | 17.07 | 7.75 | 0.002 | 82.6 | 0 |
