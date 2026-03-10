.PHONY: build release examples clean help install uninstall check fmt clippy sign

help:
	@echo "TDB - Timeless Debugger for macOS"
	@echo ""
	@echo "Available targets:"
	@echo "  build      - Build debug version"
	@echo "  release    - Build optimized release version (auto-signs)"
	@echo "  examples   - Build example C programs"
	@echo "  clean      - Remove build artifacts"
	@echo "  install    - Install to /usr/local/bin"
	@echo "  uninstall  - Remove from /usr/local/bin"
	@echo "  check      - Run cargo check"
	@echo "  fmt        - Run cargo fmt"
	@echo "  clippy     - Run cargo clippy"
	@echo "  sign       - Re-sign the release binary"
	@echo "  help       - Show this help"
	@echo ""
	@echo "Quick start:"
	@echo "  make release"
	@echo "  sudo ./target/release/tdb run ./examples/fast_test trace.tdb"
	@echo "  ./target/release/tdb tui trace.tdb"

build:
	cargo build

release:
	cargo build --release
	@echo ""
	@echo "Signing binary with entitlements..."
	codesign -s - --entitlements entitlements.plist -f ./target/release/tdb
	@echo ""
	@echo "Binary built and signed: ./target/release/tdb"
	@echo ""
	@echo "Quick start:"
	@echo "  sudo ./target/release/tdb run ./examples/fast_test trace.tdb"
	@echo "  ./target/release/tdb tui trace.tdb"
	@echo "  ./target/release/tdb view trace.tdb"

examples:
	@mkdir -p examples
	gcc examples/fast_test.c -o examples/fast_test -g
	gcc examples/test_calls.c -o examples/test_calls -g
	gcc examples/vulnerable.c -o examples/vulnerable -g -fno-stack-protector
	gcc examples/complex.c -o examples/complex -g -O0
	@echo "Examples built: fast_test test_calls vulnerable complex"

clean:
	cargo clean
	rm -f examples/fast_test examples/test_calls examples/vulnerable *.tdb

install: release
	@echo "Installing to /usr/local/bin/tdb"
	sudo cp target/release/tdb /usr/local/bin/tdb
	@echo "Done! Run 'tdb' from anywhere"

uninstall:
	sudo rm -f /usr/local/bin/tdb

check:
	cargo check

fmt:
	cargo fmt

clippy:
	cargo clippy

sign:
	@echo "Signing binary with entitlements..."
	codesign -s - --entitlements entitlements.plist -f ./target/release/tdb
	@echo "Binary signed."
