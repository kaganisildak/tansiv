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

DEST_LIB_DIR := $(PREFIX)/lib
DEST_INC_DIR := $(PREFIX)/include

.PHONY: build
build:
	$(CARGO) build $(CARGO_FLAGS)

.PHONY: test
test:
	$(CARGO) test $(CARGO_FLAGS) -- --test-threads 1 $(TEST_FLAGS)

.PHONY: install
install: build
	install -d -m 755 $(DEST_LIB_DIR)
	install -d -m 755 $(DEST_INC_DIR)
	install -m 0644 -t $(DEST_LIB_DIR) $(OUT_DIR)/libfake_vm.a
	install -m 0644 -t $(DEST_INC_DIR) $(TOP_DIR)/capi/src/fake_vm.h
