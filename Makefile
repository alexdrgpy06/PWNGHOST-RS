# PWNGHOST-RS Makefile

.PHONY: build image test clean

# Default target
all: build

# Cross-compile for both Pi Zero W (ARMv6) and Pi Zero 2W (ARMv7)
build:
	cargo build --release --target arm-unknown-linux-gnueabihf --workspace
	cargo build --release --target armv7-unknown-linux-gnueabihf --workspace

# Build for Pi Zero W only
build-armv6:
	cargo build --release --target arm-unknown-linux-gnueabihf --workspace

# Build for Pi Zero 2W only
build-armv7:
	cargo build --release --target armv7-unknown-linux-gnueabihf --workspace

# Run all tests
test:
	cargo test --workspace

# Run tests with cross-compilation
test-armv6:
	cargo test --target arm-unknown-linux-gnueabihf --workspace

test-armv7:
	cargo test --target armv7-unknown-linux-gnueabihf --workspace

# Build SD card image (requires Docker and pi-gen)
image:
	docker build -t pwnghost-rs-builder -f Dockerfile.builder .
	docker run --rm --privileged \
		-v $(PWD)/pi-gen:/pi-gen \
		-v $(PWD)/build:/build \
		pwnghost-rs-builder \
		/bin/bash -c "cd /pi-gen && ./build.sh"

# Build image locally (requires pi-gen dependencies)
image-local:
	cd pi-gen && ./build.sh

# Check formatting
fmt:
	cargo fmt --all --check

# Run clippy
clippy:
	cargo clippy --workspace -- -D warnings

# Full CI pipeline
ci: fmt clippy test build

# Clean build artifacts
clean:
	cargo clean
	rm -rf build/

# Install cross-compilation targets
install-targets:
	rustup target add arm-unknown-linux-gnueabihf armv7-unknown-linux-gnueabihf

# Setup cross-compilation environment
setup-cross:
	apt-get update && apt-get install -y \
		gcc-arm-linux-gnueabihf \
		gcc-arm-linux-gnueabihf \
		libc6-dev-armhf-cross \
		libstdc++-12-dev-armhf-cross \
		pkg-config \
		libssl-dev:armhf \
		libudev-dev:armhf \
		libsqlite3-dev:armhf

# Generate Cargo.lock
lock:
	cargo generate-lockfile

# Update dependencies
update:
	cargo update

# Build documentation
doc:
	cargo doc --workspace --no-deps

# Check for security vulnerabilities
audit:
	cargo audit

# Show outdated dependencies
outdated:
	cargo outdated

# Generate SBOM
sbom:
	cargo sbom

# Default help
help:
	@echo "PWNGHOST-RS Makefile targets:"
	@echo "  build        - Cross-compile for ARMv6 and ARMv7"
	@echo "  build-armv6  - Cross-compile for Pi Zero W (ARMv6)"
	@echo "  build-armv7  - Cross-compile for Pi Zero 2W (ARMv7)"
	@echo "  test         - Run all tests"
	@echo "  image        - Build SD card image (requires Docker)"
	@echo "  image-local  - Build SD card image locally"
	@echo "  fmt          - Check formatting"
	@echo "  clippy       - Run clippy lints"
	@echo "  ci           - Full CI pipeline"
	@echo "  clean        - Clean build artifacts"
	@echo "  install-targets - Install Rust cross-compilation targets"
	@echo "  setup-cross  - Setup cross-compilation environment (Debian/Ubuntu)"