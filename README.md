# mac-stats

A macOS terminal monitor for the stats you glance at in the menu bar: CPU, memory, disk, and network. It is one local binary. It does not phone home, write a config file, or keep history after you quit.

The screen refreshes about once a second. Five views share that sample:

| Key | View | What it shows |
| --- | --- | --- |
| `5` | Essentials | The headline of each of the other four, with no process table |
| `1` | RAM | Used memory, the app / wired / compressed / free split, swap, history, top processes |
| `2` | CPU | Overall, user, system, and idle time, cores, load, uptime, history, top processes |
| `3` | Disk | Each user volume's free space and read/write rate, history, top processes |
| `4` | Network | The default interface, down and up rates, history, totals since launch |

Essentials uses a two-by-two layout when the content area is at least 88 columns by 16 rows. The title and the footer take three rows, so that is a terminal of about 88 by 19. Otherwise the four cards stack.

## Requirements

- macOS. The collectors read Mach, IOKit, and sysctl, and the crate does not build on other systems.
- A current stable Rust toolchain (`rustup` is the straightforward way to get one). The package uses the 2024 edition.
- A terminal at least 70 columns by 20 rows. A smaller window asks you to resize it.

## Run

From this directory:

```shell
cargo run
```

That opens the terminal UI on Essentials. A release build is the one to leave running:

```shell
cargo build --release
./target/release/mac-stats
```

Print one sample and exit. The program takes a first reading, waits one second, then prints the second, so the rates in that report are real:

```shell
cargo run -- --once
```

Show the key list:

```shell
cargo run -- --help
```

An unknown argument exits with status 2.

### Keys

| Key | Action |
| --- | --- |
| `5` `1` `2` `3` `4` | Jump to a view |
| Tab, Right | Next view, in the order Essentials, RAM, CPU, Disk, Network |
| Shift-Tab, Left | Previous view |
| `q`, Esc, Ctrl-C | Quit |

Rates show `…` until the second sample, then `—` when a counter is missing.

## What each view measures

**RAM.** Installed memory comes from `hw.memsize`. The split follows the same parts as the Stats menu-bar app: app, wired, compressed, and free. Those four add up to installed RAM. The bar and the history are percent of that total. Swap comes from `vm.swapusage`. The process list is the top 8 by resident memory. Resident size can differ from the physical footprint Activity Monitor shows.

**CPU.** Overall, user, system, and idle come from host CPU tick deltas. Nice time is counted as user. On Apple Silicon, efficiency and performance cores are labeled when the two performance levels add up to the core count. Per-core bars use the same tick counters. Load is the 1, 5, and 15 minute averages, and uptime is the system uptime. The process list is the top 8 by CPU. A process can read above 100% because that figure is one core, not the whole chip. Live GHz is not shown. The public sysctls report 0 on Apple Silicon.

**Disk.** Volumes are the startup disk (`/`) and anything mounted directly under `/Volumes`. Hidden system mounts such as `/System/Volumes/Data` are left out. Each card shows free space, a capacity gauge, and read/write throughput. Throughput comes from the IOKit block-storage counters for that volume's disk. Volumes on the same physical disk share those counters. The graph draws read (blue) and write (red) from the same baseline. The process list is the top 8 by disk IO since the last sample.

**Network.** The interface is the one used for the default route, found by matching a local UDP connect against the interface addresses. The card shows whether it is up, the local IPv4 address, the MAC address, and the current down and up rates. Download is blue and grows down from the center line. Upload is red and grows up from it. Both use one scale, so the same rate is the same height on either side. "Since launch" is the sum of samples taken while this process has been running, and it resets if the chosen interface changes. It is not the total since boot.

**Essentials.** The same four headlines in one screen: CPU percent and history, the memory split, mounted user volumes, and the default interface. The Disk card lists each volume with free space and read/write rates, adding per-volume history when space allows. Smaller cards use compact summaries and show how many additional volumes are available in the Disk view. There is no process table on this view.

## Graphs

History is kept in memory for about 10 minutes (600 one-second samples). Nothing is written to disk.

Each sample is one column, with the newest column on the right. A short trace stays on the right instead of being stretched across the window. CPU and memory use a fixed 0–100% axis. Disk and network use a 1-2-5 axis that rises as soon as traffic crosses the next step and steps back down slowly after a spike has left the history, so a bar keeps its height while it scrolls left.

## Not in this version

Temperature, the memory-pressure gauge, live clock speed, ping latency and jitter, and per-process network rates are left out. The numbers above are the ones macOS exposes directly.

## Develop

```shell
cargo test
cargo clippy --all-targets -- -D warnings
```

`cargo test` includes a live sample of this Mac. The UI tests render the five views without opening a terminal.
