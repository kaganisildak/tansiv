BUILD_TESTS := build_test
RUN_TESTS :=

ifeq ($(OUT_DIR),)
$(error "OUT_DIR not set")
else
ifeq ($(patsubst /%, %, $(OUT_DIR)),$(OUT_DIR))
$(error "OUT_DIR=$(OUT_DIR): not an absolute path")
endif
endif

TARGET_BUILD_TESTS := $(patsubst %, $(OUT_DIR)/%, $(BUILD_TESTS))
TARGET_RUN_TESTS := $(patsubst %, $(OUT_DIR)/%, $(RUN_TESTS))

.PHONY: test
test: $(TARGET_BUILD_TESTS) $(TARGET_RUN_TESTS)
	for test in $(TARGET_RUN_TESTS); do $$test || { echo "test $$test failed!"; exit 1; }; done

$(TARGET_BUILD_TESTS) $(TARGET_RUN_TESTS): $(OUT_DIR)/%: %.c ../../capi/src/tansiv-client.h $(OUT_DIR)/libtanproc.a
	gcc -Wall -o $@ -I../../capi/src -L$(OUT_DIR) $< -ltanproc -lpthread -ldl -lrt -lm
