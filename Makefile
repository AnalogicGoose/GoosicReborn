.PHONY: build-service test-rust test-rust-live build-swift test-swift build-linux test-linux run-linux test-ui-macos debug-bundle-macos package-macos run-service run-swift test

build-service:
	cargo build -p goosic-service

test-rust:
	cargo test --workspace

# Hits music.youtube.com. Kept out of `test` so the default run stays offline and deterministic.
test-rust-live:
	cargo test --workspace -- --ignored --nocapture --test-threads=1

UNAME_S := $(shell uname -s)

# The Swift package now contains only the native macOS app.
SWIFT_SCRATCH := $(HOME)/Library/Caches/goosic-swift-build
GOOSIC_MACOS_SDK := $(shell xcrun --sdk macosx --show-sdk-version 2>/dev/null)
SWIFT_ENV := GOOSIC_MACOS_SDK=$(GOOSIC_MACOS_SDK)

SWIFT_FLAGS := --package-path apps/goosic-swift --scratch-path $(SWIFT_SCRATCH)

build-swift:
	$(SWIFT_ENV) swift build $(SWIFT_FLAGS)

test-swift:
	$(SWIFT_ENV) swift test $(SWIFT_FLAGS)

build-linux: build-service
	cargo build --manifest-path apps/goosic-linux/Cargo.toml

test-linux: build-service
	cargo test --manifest-path apps/goosic-linux/Cargo.toml

run-linux: build-linux
	GOOSIC_SERVICE_PATH="$(CURDIR)/target/debug/goosic-service" cargo run --manifest-path apps/goosic-linux/Cargo.toml

# Runs the real macOS shell against local fixture data, including scroll and account-control UI
# checks. XcodeGen is used only to materialize the disposable Xcode UI-test host.
test-ui-macos:
	@test "$(UNAME_S)" = Darwin || (echo "macOS UI tests require macOS" >&2; exit 2)
	cd tools/macos-ui-tests && xcodegen generate
	xcodebuild test -project tools/macos-ui-tests/GoosicMacUITests.xcodeproj -scheme GoosicDebugHost -destination 'platform=macOS'

debug-bundle-macos:
	@test "$(UNAME_S)" = Darwin || (echo "Debug bundles require macOS" >&2; exit 2)
	sh tools/debug-bundle-macos.sh

# The download a tester installs. See docs/RELEASING.md.
package-macos:
	@test "$(UNAME_S)" = Darwin || (echo "macOS app bundles are built on macOS" >&2; exit 2)
	sh tools/package-macos.sh $(VERSION)

ifeq ($(UNAME_S),Darwin)
test: test-rust test-swift
else
test: test-rust
endif

run-service:
	@test -n "$(GOOSIC_SERVICE_PATH)" || (echo "set GOOSIC_SERVICE_PATH to the service executable" >&2; exit 2)
	"$(GOOSIC_SERVICE_PATH)"

run-swift: build-service
	GOOSIC_SERVICE_PATH="$(CURDIR)/target/debug/goosic-service" $(SWIFT_ENV) swift run $(SWIFT_FLAGS) goosic-swift
