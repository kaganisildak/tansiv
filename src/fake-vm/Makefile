CARGO := cargo
CARGO_FLAGS ?=
TEST_FLAGS ?=
PREFIX ?= /usr/local

TOP_DIR := $(shell pwd)

ifeq ($(RELEASE),)
	PROFILE := debug
else
	PROFILE := release
	CARGO_FLAGS += --release
endif
OUT_DIR := $(TOP_DIR)/target/$(PROFILE)

TEST_DIRS := $(TOP_DIR)/fake_vm_capi/tests

DEST_LIB_DIR := $(PREFIX)/lib
DEST_INC_DIR := $(PREFIX)/include

.PHONY: build
build:
	$(CARGO) build $(CARGO_FLAGS)

.PHONY: test
test: cargo-tests other-tests

.PHONY: cargo-tests
cargo-tests:
	$(CARGO) test --workspace $(CARGO_FLAGS) -- --test-threads 1 $(TEST_FLAGS)

.PHONY: other-tests $(TEST_DIRS)
other-tests: $(TEST_DIRS)

$(TEST_DIRS):
	$(MAKE) -C $@ test OUT_DIR=$(OUT_DIR)

.PHONY: install
install: build
	install -d -m 755 $(DEST_LIB_DIR)
	install -d -m 755 $(DEST_INC_DIR)
	install -m 0644 -t $(DEST_LIB_DIR) $(OUT_DIR)/libfake_vm.a
	install -m 0644 -t $(DEST_INC_DIR) $(TOP_DIR)/capi/src/fake_vm.h
