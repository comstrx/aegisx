# validate

workers=4 threads=2 seconds=4 trials=1 host=Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39

## 64 connections

| target | req/s | p50 ms | p99 ms | cpu µs/req | ctx sw/req | syscalls/req | pss MB | errors |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| direct | 24197.1 | 0.83 | 25.55 | 22.04 | 0.417 | 0.0 | 0.4 | 0 |
| nginx | 12168.0 | 4.3 | 57.76 | 65.8 | 0.792 | 2.003 | 31.0 | 0 |
| pingora | 3505.1 | 15.76 | 82.95 | 223.84 | 0.342 | 0.004 | 12.4 | 0 |
| aegisx | 26191.4 | 1.5 | 43.3 | 63.53 | 0.355 | 1.051 | 10.4 | 0 |
