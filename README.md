# mac-stats

A macOS terminal monitor for CPU, memory, disk, and network.

```shell
cargo run
```

Press `5` for the combined essentials view, `1` for RAM, `2` for CPU, `3` for disk, and `4` for network. Tab and the arrow keys also switch views. `q` quits.

`cargo run -- --once` prints one sample and exits. Rates appear after the second sample, about a second in.

The first version shows the counters macOS exposes directly: CPU user, system, and idle time, efficiency and performance cores on Apple Silicon, the app / wired / compressed / free memory split, swap, per-volume free space and throughput, and the default network interface. Temperature, the memory-pressure gauge, ping latency, and per-process network rates are not in this version.
