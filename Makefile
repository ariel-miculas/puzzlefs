SRC=$(shell find . -name \*.rs | grep -v "^./target")
PREFIX?=/usr/local
ROOT_SBINDIR?=$(PREFIX)/sbin
INSTALL=install


.PHONY: debug
debug:
	cargo build

.PHONY: release
release:
	cargo build --release

.PHONY: check
check:
	RUST_BACKTRACE=1 cargo test -- --nocapture

.PHONY: lint
lint: $(SRC)
	rustfmt --check $(SRC)
	cargo clippy --all-targets --all-features -- -D warnings -D rust-2018-idioms -D rust-2021-compatibility -A clippy::upper-case-acronyms

.PHONY: fmt
fmt:
	rustfmt --emit files $(SRC)

.PHONY: clean
clean:
	-cargo clean

install:
	$(INSTALL) -m0755 -D target/debug/puzzlefs -t $(ROOT_SBINDIR)
