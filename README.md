# TDB

<img width="1919" height="1037" alt="Screenshot 2025-11-08 at 3 16 06 AM" src="https://github.com/user-attachments/assets/4f7ac48d-a0f9-4ddb-98f1-c8cc40b33807" />

Timeless debugger for macOS. Records complete execution trace.

## Build

```bash
cargo build --release
```

## Usage

Trace a program:
```bash
tdb run ./program output.tdb
tdb run python3 script.py trace.tdb
```

Attach to running process:
```bash
tdb trace <pid> output.tdb
```

View trace in browser:
```bash
tdb view trace.tdb
```
Open http://localhost:8080

Show statistics:
```bash
tdb stats trace.tdb
```

## Keyboard Shortcuts

- `h` / `left` - previous step
- `l` / `right` - next step  
- `g` - first step
- `G` - last step
- `c` - next call
- `r` - next return
- `m` - next memory change
- `/` - search

## Requirements

macOS only. Requires root permissions for process tracing.

