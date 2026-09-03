.PHONY: build-service test-rust test-rust-live build-swift test-swift run-service run-swift test

build-service:
	cargo build -p goosic-service

test-rust:
	cargo test --workspace

# Hits music.youtube.com. Kept out of `test` so the default run stays offline and deterministic.
test-rust-live:
	cargo test --workspace -- --ignored --nocapture --test-threads=1

# The repository lives under a file-provider-synced directory, which stamps
# `com.apple.FinderInfo` onto build products and makes codesign refuse to sign the test bundle.
# Building outside that directory avoids the problem entirely.
SWIFT_SCRATCH := $(HOME)/Library/Caches/goosic-swift-build
SWIFT_FLAGS := --package-path apps/goosic-swift --scratch-path $(SWIFT_SCRATCH)

build-swift:
	SCUI_DEFAULT_BACKEND=AppKitBackend swift build $(SWIFT_FLAGS)

test-swift:
	SCUI_DEFAULT_BACKEND=AppKitBackend swift test $(SWIFT_FLAGS)

test: test-rust test-swift

run-service:
	@test -n "$(GOOSIC_SERVICE_PATH)" || (echo "set GOOSIC_SERVICE_PATH to the service executable" >&2; exit 2)
	"$(GOOSIC_SERVICE_PATH)"

run-swift: build-service
	GOOSIC_SERVICE_PATH="$(CURDIR)/target/debug/goosic-service" SCUI_DEFAULT_BACKEND=AppKitBackend swift run $(SWIFT_FLAGS) goosic-swift
