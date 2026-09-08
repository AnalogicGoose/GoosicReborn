.PHONY: build-service test-rust test-rust-live build-swift build-swift-portable test-swift \
        run-service run-swift test

build-service:
	cargo build -p goosic-service

test-rust:
	cargo test --workspace

# Hits music.youtube.com and lrclib.net. Kept out of `test` so the default run stays offline
# and deterministic.
test-rust-live:
	cargo test --workspace -- --ignored --nocapture --test-threads=1

UNAME_S := $(shell uname -s)

ifeq ($(UNAME_S),Darwin)
# The macOS backend is named explicitly because the SwiftCrossUI package also exposes optional
# non-macOS backends. Elsewhere DefaultBackend picks the right one for the host (GTK on Linux,
# WinUI on Windows), and naming AppKitBackend there would try to build it and fail.
SWIFT_BACKEND := SCUI_DEFAULT_BACKEND=AppKitBackend
# This repository lives under a file-provider-synced directory, which stamps
# `com.apple.FinderInfo` onto build products and makes codesign refuse to sign the test bundle.
# Building outside that directory avoids the problem entirely. Only macOS codesigns, so only
# macOS needs the detour.
SWIFT_SCRATCH := --scratch-path $(HOME)/Library/Caches/goosic-swift-build
else
SWIFT_BACKEND :=
SWIFT_SCRATCH :=
endif

SWIFT_FLAGS := --package-path apps/goosic-swift $(SWIFT_SCRATCH)

build-swift:
	$(SWIFT_BACKEND) swift build $(SWIFT_FLAGS)

# Compiles the non-macOS code paths on whatever host you are on.
#
# The Linux and Windows branches of the platform hosts are `#else` blocks, so on macOS they are
# never type-checked and can silently drift from the API the shell calls — which is exactly how
# they broke. This target forces them to compile, so that drift is caught here rather than on a
# Linux machine.
build-swift-portable:
	$(SWIFT_BACKEND) swift build $(SWIFT_FLAGS) \
		--scratch-path $(HOME)/.cache/goosic-swift-portable -Xswiftc -DGOOSIC_PORTABLE

test-swift:
	$(SWIFT_BACKEND) swift test $(SWIFT_FLAGS)

test: test-rust test-swift build-swift-portable

run-service:
	@test -n "$(GOOSIC_SERVICE_PATH)" || (echo "set GOOSIC_SERVICE_PATH to the service executable" >&2; exit 2)
	"$(GOOSIC_SERVICE_PATH)"

run-swift: build-service
	GOOSIC_SERVICE_PATH="$(CURDIR)/target/debug/goosic-service" \
		$(SWIFT_BACKEND) swift run $(SWIFT_FLAGS) goosic-swift
