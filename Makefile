.PHONY: build release test examples clean run-test help

help:
	@echo "TDB - Timeless Debugger for macOS"
	@echo ""
	@echo "Available targets:"
	@echo "  build      - Build debug version"
	@echo "  release    - Build optimized release version"
	@echo "  test       - Build test programs"
	@echo "  examples   - Build example programs"
	@echo "  clean      - Remove build artifacts"
	@echo "  run-test   - Run test program"
	@echo "  help       - Show this help"

build:
	cargo build

release:
	cargo build --release
	@echo ""
	@echo "Binary: ./target/release/tdb"

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

