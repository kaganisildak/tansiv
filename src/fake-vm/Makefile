CARGO := cargo
CARGO_FLAGS ?=
TEST_FLAGS ?=

.PHONY: build
build:
	$(CARGO) build $(CARGO_FLAGS)

.PHONY: test
test:
	$(CARGO) test $(CARGO_FLAGS) -- --test-threads 1 $(TEST_FLAGS)
