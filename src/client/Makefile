LIB_CRATES := tanproc_capi tanqemu_capi tanqemukvm_capi tanqemuxen_capi

CARGO := cargo
CARGO_ENV ?=
CARGO_FLAGS ?=
TEST_FLAGS ?=
PREFIX ?= /usr/local

TOP_DIR := $(shell pwd)

QEMU_SRC ?= $(TOP_DIR)/../qemu
QEMU_BUILD ?= $(TOP_DIR)/../qemu/build

CARGO_ENV += QEMU_SRC=$(QEMU_SRC) QEMU_BUILD=$(QEMU_BUILD)

ifeq ($(RELEASE),)
	PROFILE := debug
else
	PROFILE := release
	CARGO_FLAGS += --release
endif
OUT_DIR := $(TOP_DIR)/target/$(PROFILE)

TEST_DIRS := $(TOP_DIR)/tanproc_capi/tests

DEST_LIB_DIR := $(PREFIX)/lib
DEST_INC_DIR := $(PREFIX)/include

.PHONY: build
build: $(LIB_CRATES)

.PHONY: $(LIB_CRATES)
$(LIB_CRATES):
	$(CARGO_ENV) $(CARGO) build -p $@  $(CARGO_FLAGS)

.PHONY: test
test: cargo-tests other-tests

.PHONY: cargo-tests
cargo-tests:
	$(CARGO_ENV) $(CARGO) test --workspace $(CARGO_FLAGS) -- --test-threads 1 $(TEST_FLAGS)

.PHONY: other-tests $(TEST_DIRS)
other-tests: $(TEST_DIRS)

$(TEST_DIRS):
	$(MAKE) -C $@ test OUT_DIR=$(OUT_DIR)

.PHONY: install
install: build
	install -d -m 755 $(DEST_LIB_DIR)
	install -d -m 755 $(DEST_INC_DIR)
	install -m 0644 -t $(DEST_LIB_DIR) $(OUT_DIR)/libtanproc.a
	install -m 0644 -t $(DEST_LIB_DIR) $(OUT_DIR)/libtanqemu.a
	install -m 0644 -t $(DEST_LIB_DIR) $(OUT_DIR)/libtanqemukvm.a
	install -m 0644 -t $(DEST_LIB_DIR) $(OUT_DIR)/libtanqemuxen.a
	install -m 0644 -t $(DEST_INC_DIR) $(TOP_DIR)/capi/src/tansiv-client.h
