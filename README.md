# TDB

<img width="1919" height="1037" alt="Screenshot 2025-11-08 at 3 16 06 AM" src="https://github.com/user-attachments/assets/4f7ac48d-a0f9-4ddb-98f1-c8cc40b33807" />

Timeless debugger for macOS. Records complete execution trace.

## Build

### Method 1: Using Make (Recommended)

```bash
make release
```

This automatically builds the binary and signs it with the necessary entitlements.

### Method 2: Manual Build

```bash
# Build the binary
cargo build --release

# Sign with entitlements (REQUIRED on macOS)
codesign -s - --entitlements entitlements.plist -f ./target/release/tdb
```

**⚠️ Important:** The binary MUST be code-signed with entitlements to attach to processes on macOS. If you see `task_for_pid failed: 5`, you need to sign the binary.

## Usage Examples

### 1. Trace a Python Script

```bash
# Your existing Python script - no modifications needed!
cd ~/projects/my-app
sudo tdb run python3 main.py trace.tdb

# View the trace
tdb view trace.tdb  # Opens server at http://localhost:8080
```

### 2. Trace a Compiled Binary

```bash
# Trace your C/C++/Rust/etc program
cd ~/projects/my-binary
sudo tdb run ./my_program trace.tdb

# With arguments
sudo tdb run ./my_program --config config.json trace.tdb
```

### 3. Attach to Running Process

If you have a long-running process:

```bash
# Find the PID
ps aux | grep my_program

# Attach and trace (press Ctrl+C to stop)
sudo tdb trace <PID> trace.tdb
```

### 4. Analyze Trace Data

```bash
# View statistics
tdb stats trace.tdb

# Interactive browser view
tdb view trace.tdb [port]
# Then open http://localhost:8080 (or your specified port)
```

## What TDB Captures

For every instruction executed:
- **Program Counter (PC)** - exact address
- **Instruction** - disassembled with operands
- **All CPU Registers** - complete state (x86_64 or ARM64)
- **Memory Changes** - stack and heap modifications
- **Call/Return Detection** - function call depth tracking

## Use Cases

- **Debugging**: Step backward through execution to find where bugs occur
- **Reverse Engineering**: Understand how programs work at the instruction level
- **Security Analysis**: Trace malware or vulnerabilities in a controlled environment
- **Performance Analysis**: See exactly what code is executing
- **Learning**: Watch how high-level code translates to assembly

## Keyboard Shortcuts (in web viewer)

- `h` / `←` - previous step
- `l` / `→` - next step  
- `g` - first step
- `G` - last step
- `c` - next call
- `r` - next return
- `m` - next memory change
- `/` - search

## Troubleshooting

### Error: "task_for_pid failed: 5"

**Cause:** The binary is not code-signed with proper entitlements.

**Solution:**
```bash
codesign -s - --entitlements entitlements.plist -f ./target/release/tdb
```

Or rebuild using `make release` which automatically signs the binary.

### Error: "Permission denied" or "Operation not permitted"

**Cause:** Process tracing requires root permissions on macOS.

**Solution:** Run TDB with `sudo`:
```bash
sudo ./target/release/tdb run python3 script.py trace.tdb
```

### Traced program exits too quickly

**This is now fixed!** TDB automatically pauses the program using `SIGSTOP` before attaching, then resumes it once the tracer is ready.

You can trace even the fastest programs without modifications:
```bash
sudo ./target/release/tdb run ./fast_program trace.tdb
```

## Requirements

- macOS only (uses Mach kernel APIs)
- Root/sudo permissions for process tracing
- Code signing with entitlements (handled by `make release`)

## Installation (Optional)

Install system-wide:
```bash
make install
```

Then use `tdb` from anywhere:
```bash
sudo tdb run python3 script.py trace.tdb
```

## Complete Example

Here's a complete workflow tracing your own program:

```bash
# 1. Install TDB
git clone https://github.com/yourusername/tdb.git
cd tdb
make install

# 2. Go to your project
cd ~/my-project

# 3. Trace your program (no modifications needed!)
sudo tdb run python3 my_script.py my_trace.tdb
# or
sudo tdb run ./my_compiled_app my_trace.tdb

# 4. Analyze the trace
tdb stats my_trace.tdb

# 5. View interactively in browser
tdb view my_trace.tdb
# Open http://localhost:8080
# Use keyboard shortcuts to navigate through execution history
```

### Example with a simple Python script:

```python
# my_script.py - Your regular code, no changes needed!
def calculate(x, y):
    return x * y + 10

result = calculate(5, 3)
print(f"Result: {result}")
```

```bash
# Trace it
sudo tdb run python3 my_script.py trace.tdb

# Output:
# Launching: python3 my_script.py
# Process started with PID: 12345 (paused)
# 📍 Attaching to process PID: 12345
# ✅ Tracer attached, process resumed
# Result: 25
# Total Steps: 150 (example)

# View the trace
tdb view trace.tdb
```

Now navigate through every instruction that executed, see all register states, and step backward/forward through execution!

