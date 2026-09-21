# Native loopback latency

The opt-in Rust benchmark compares the same synthetic 32 KiB JSON request and
response directly against a local echo server and through the production gateway.
It includes the Windows TCP peer verification listener, request classification,
native forwarding, response streaming and byte-for-byte result checks. No browser,
OpenAI endpoint, installed configuration or real authentication is used.

Run in a release build:

    cargo test --locked --release -p cxweb-runtime host::latency_benchmark::paired_native_loopback_latency -- --ignored --nocapture

Optionally set `CXWEB_BENCHMARK_REPORT` to an absolute, nonexistent JSON filename.
The report is written with create-new semantics. Record the machine, OS, power
mode, concurrent activity and benchmark executable hash with the result.

The benchmark warms both paths for 100 pairs, then measures 1,000 pairs with
alternating order and persistent HTTP connections. Its incremental statistic is
the gateway duration minus the direct duration for each pair. Negative differences
are retained. p95 uses nearest rank; cold figures are only the first request on
each path and cannot establish a cold-start percentile.

The September 21 workstation run in `loopback-sep21.json` observed an incremental
median of 0.1971 ms and p95 of 0.3255 ms, below the PRD's 20 ms p95 target for this
fixture. The account-live cohort and desktop were active; this is a shared-machine
observation, not an isolated release benchmark. It does not qualify WebSocket
overhead, fresh-connection churn, memory, idle CPU, browser latency or model speed.
