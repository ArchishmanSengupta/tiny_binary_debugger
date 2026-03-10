# TDB - Timeless Debugger for macOS

<img width="1919" height="1037" alt="Screenshot 2025-11-08 at 3 16 06 AM" src="https://github.com/user-attachments/assets/4f7ac48d-a0f9-4ddb-98f1-c8cc40b33807" />

A timeless debugger that records the complete execution trace of any program, instruction by instruction. Step forward and backward through time to understand exactly what happened.

**98 tests. Zero warnings. Zero clippy lints.**

## How It Works

TDB uses `ptrace(PT_STEP)` for true single-step execution and Mach kernel APIs for reading registers and memory. Every instruction that executes is recorded with full CPU state, so you can navigate the entire execution history after the fact.

For every instruction executed, TDB captures:
- **Program Counter (PC)** - exact address
- **Instruction** - disassembled via Capstone (ARM64 / x86_64)
- **All CPU Registers** - complete state snapshot
- **Memory Changes** - byte-level stack modifications
- **Call/Return Detection** - function call depth tracking

## Quick Start

```bash
# Build and sign
make release

# Trace a program
sudo ./target/release/tdb run ./examples/complex trace.tdb

# View the trace (pick one)
./target/release/tdb tui trace.tdb       # Terminal UI
./target/release/tdb view trace.tdb      # Browser at http://localhost:8080
./target/release/tdb stats trace.tdb     # Statistics summary
```

## Build

### Using Make (Recommended)

```bash
make release
```

Builds the binary and signs it with the required debugger entitlements automatically.

### Manual Build

```bash
cargo build --release
codesign -s - --entitlements entitlements.plist -f ./target/release/tdb
```

The binary **must** be code-signed with entitlements to use `task_for_pid` on macOS.

### Build Examples

```bash
make examples
```

Builds `fast_test`, `test_calls`, `vulnerable`, and `complex` in the `examples/` directory.

## Usage

### Trace a Program

```bash
# Run and trace any binary
sudo tdb run ./my_program trace.tdb

# With arguments (trace file is always last)
sudo tdb run ./my_program --flag arg1 arg2 trace.tdb

# Trace a Python script
sudo tdb run python3 script.py trace.tdb
```

### Attach to a Running Process

```bash
sudo tdb trace <PID> trace.tdb
```

Press Ctrl+C to stop tracing. The trace is always saved before exit.

### Terminal UI (TUI)

```bash
tdb tui trace.tdb
```

Full terminal viewer with vim-style navigation:

| Key | Action |
|-----|--------|
| `j` / `k` | Move down / up one step |
| `Ctrl+d` / `Ctrl+u` | Page down / up |
| `g` / `G` | Jump to first / last step |
| `/` | Search instructions |
| `n` / `N` | Next / previous search match |
| `c` | Jump to next CALL |
| `r` | Jump to next RETURN |
| `m` | Jump to next memory change |
| `Tab` | Cycle focused panel |
| `q` | Quit |
| `?` | Help overlay |

### Web Viewer

```bash
tdb view trace.tdb [port]
```

Opens a web server (default port 8080) with:
- Virtual-scrolling instruction list (handles millions of steps)
- Register state viewer per step
- Memory change viewer
- Search with forward/reverse find
- Keyboard navigation

### Statistics

```bash
tdb stats trace.tdb
```

```
  Trace Statistics
  ----------------
  Total steps:      8432
  Unique addresses: 312
  Function calls:   127
  Returns:          127
  Jumps/branches:   1843
  Memory changes:   3204
  Most executed:    0x19bca1020 (89 times)

  Top instructions:
    mov                89
    add                76
    ldr                68
    str                54
    ...
```

## Examples

### fast_test

A simple program with printf and a loop. Quick to trace, good for sanity checks.

```bash
sudo tdb run ./examples/fast_test trace.tdb
```

### complex

Exercises recursion (fibonacci), quicksort, hash tables, linked lists, matrix multiplication, and recursive GCD. Generates a rich trace with deep call stacks and lots of memory changes.

```bash
sudo tdb run ./examples/complex trace.tdb
```

### test_calls

Multiple function calls with distinct call/return patterns.

```bash
sudo tdb run ./examples/test_calls trace.tdb
```

## Architecture

```
src/
  main.rs           CLI entry point, trace loop, Ctrl+C handling
  launcher/mod.rs   fork() + ptrace(PT_TRACE_ME) + execvp() launcher
  tracer/
    trace.rs        Single-step engine (ptrace PT_STEP + waitpid)
    mach.rs         Mach APIs (task_for_pid, thread_get_state, vm_read)
  storage/mod.rs    In-memory BTreeMap + bincode save/load
  stats/mod.rs      Trace analysis (calls, branches, memory, top insns)
  server/mod.rs     Axum web server with embedded HTML
  tui.rs            Ratatui terminal UI viewer
web/
  index.html        Single-file web viewer (embedded in binary)
tests/
  cli.rs            CLI integration tests
examples/
  fast_test.c       Simple test program
  test_calls.c      Function call patterns
  complex.c         Recursion, sorting, hashing, linked lists, matrices
  vulnerable.c      Buffer overflow demo
```

### Design Decisions

- **ptrace for control, Mach for observation**: ptrace handles single-stepping and process lifecycle. Mach APIs read registers and memory (richer interface than ptrace on macOS).
- **fork + PT_TRACE_ME**: Eliminates the race condition between spawn and attach. The child stops at `execvp` before any user code runs.
- **BTreeMap storage**: Entries are always ordered by step number. Range queries are efficient.
- **Embedded HTML**: The web viewer is compiled into the binary via `include_str!`, so `tdb view` works from any directory.
- **Pending signal tracking**: When a signal other than SIGTRAP arrives during stepping, it's saved and re-delivered on the next `ptrace(PT_STEP)` call.

## API Endpoints

The web viewer backend exposes a JSON API:

| Endpoint | Description |
|----------|-------------|
| `GET /` | Embedded HTML viewer |
| `GET /api/trace` | All entries (or `?start=N&end=M` for range) |
| `GET /api/trace/:step` | Single entry by step number |
| `GET /api/trace/count` | Total step count |
| `GET /api/stats` | Trace statistics |

## Tests

98 tests across 6 modules:

```bash
cargo test
```

| Module | Tests | Coverage |
|--------|-------|----------|
| `storage` | 17 | Creation, insert/get, ranges, save/load, corruption, concurrency, serialization |
| `stats` | 20 | Counting, branch classification, memory changes, sorting, edge cases |
| `tracer/trace` | 25 | Store mnemonic detection (ARM64 + x86), negative cases |
| `tracer/mach` | 9 | FFI struct sizes, alignment, zero-init, flavor constants |
| `server` | 12 | All API endpoints, 404s, CORS, range queries |
| `tests/cli` | 10 | Argument validation, usage text, error handling |

## Troubleshooting

### `task_for_pid failed: 5`

The binary is not signed with debugger entitlements. Fix:

```bash
codesign -s - --entitlements entitlements.plist -f ./target/release/tdb
```

Or rebuild with `make release` which signs automatically.

### `Operation not permitted` / `Permission denied`

Process tracing requires root on macOS:

```bash
sudo ./target/release/tdb run ./my_program trace.tdb
```

### `ptrace(PT_STEP) failed: ENOTSUP`

Same as above - run with `sudo`.

## Requirements

- macOS (uses Mach kernel APIs and ptrace)
- Root/sudo for process tracing
- Rust toolchain for building
- Code signing with entitlements (handled by `make release`)

## Install / Uninstall

```bash
make install      # Copies to /usr/local/bin/tdb
make uninstall    # Removes it
```
