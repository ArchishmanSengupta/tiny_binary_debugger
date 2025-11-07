#!/bin/bash

echo "=== TDB Demo ==="
echo ""
echo "Building..."
cargo build --release

echo ""
echo "Compiling test program..."
gcc test_program.c -o test_program -g

echo ""
echo "=== Usage ==="
echo ""
echo "1. Run the test program:"
echo "   ./test_program"
echo ""
echo "2. In another terminal, trace it:"
echo "   sudo ./target/release/tdb trace <PID> trace.tdb"
echo ""
echo "3. View the trace:"
echo "   ./target/release/tdb view trace.tdb"
echo ""
echo "Then open http://localhost:8080 in your browser"

