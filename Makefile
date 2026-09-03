.PHONY: build-service test-rust test-rust-live build-swift test-swift run-service run-swift test

build-service:
	cargo build -p goosic-service

test-rust:
	cargo test --workspace

# Hits music.youtube.com. Kept out of `test` so the default run stays offline and deterministic.
test-rust-live:
	cargo test --workspace -- --ignored --nocapture --test-threads=1

build-swift:
	SCUI_DEFAULT_BACKEND=AppKitBackend swift build --package-path apps/goosic-swift

# Downloaded files pick up `com.apple.provenance`, and codesign refuses to sign a test bundle
# built from sources carrying extended attributes.
test-swift:
	xattr -cr apps/goosic-swift/Sources apps/goosic-swift/Tests apps/goosic-swift/Package.swift
	SCUI_DEFAULT_BACKEND=AppKitBackend swift test --package-path apps/goosic-swift

test: test-rust test-swift

run-service:
	@test -n "$(GOOSIC_SERVICE_PATH)" || (echo "set GOOSIC_SERVICE_PATH to the service executable" >&2; exit 2)
	"$(GOOSIC_SERVICE_PATH)"

run-swift: build-service
	GOOSIC_SERVICE_PATH="$(CURDIR)/target/debug/goosic-service" SCUI_DEFAULT_BACKEND=AppKitBackend swift run --package-path apps/goosic-swift goosic-swift
