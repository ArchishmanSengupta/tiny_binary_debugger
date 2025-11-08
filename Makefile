.PHONY: build release test examples clean run-test help install uninstall check fmt clippy sign

help:
	@echo "TDB - Timeless Debugger for macOS"
	@echo ""
	@echo "Available targets:"
	@echo "  build      - Build debug version"
	@echo "  release    - Build optimized release version (auto-signs with entitlements)"
	@echo "  test       - Build test programs"
	@echo "  examples   - Build example programs"
	@echo "  clean      - Remove build artifacts"
	@echo "  run-test   - Run test program"
	@echo "  install    - Install to /usr/local/bin"
	@echo "  help       - Show this help"
	@echo ""
	@echo "Quick start:"
	@echo "  make release"
	@echo "  sudo ./target/release/tdb run python3 examples/hello.py trace.tdb"

build:
	cargo build

release:
	cargo build --release
	@echo ""
	@echo "Signing binary with entitlements..."
	codesign -s - --entitlements entitlements.plist -f ./target/release/tdb
	@echo ""
	@echo "✅ Binary built and signed: ./target/release/tdb"
	@echo ""
	@echo "Quick start:"
	@echo "  sudo ./target/release/tdb run python3 examples/hello.py trace.tdb"

test: test_program examples

test_program:
	gcc test_program.c -o test_program -g

examples:
	@mkdir -p examples
	gcc examples/vulnerable.c -o examples/vulnerable -g -fno-stack-protector

clean:
	cargo clean
	rm -f test_program examples/vulnerable *.tdb

run-test: test_program
	./test_program

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
	@echo "✅ Binary signed successfully"

