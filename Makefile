DIETPI_HOST ?= dietpi.local
DIETPI_TARGET ?= aarch64-unknown-linux-gnu

PIZEROW_HOST ?= pizero.local
ZEROW_TARGET ?= arm-unknown-linux-musleabi

HOST ?= $(DIETPI_HOST)
TARGET ?= $(DIETPI_TARGET)
TMP_FOLDER ?= /tmp/goto

# Build and run on my RPi Zero 2W (DietPi)
replace: HOST := $(DIETPI_HOST)
replace: TARGET := $(DIETPI_TARGET)
replace: build deploy

# Build and run on my RPi Zero W (Rasbperry Pi OS Lite)
replace-zerow: HOST := $(PIZEROW_HOST)
replace-zerow: TARGET := $(ZEROW_TARGET)
replace-zerow: build-zerow deploy

install: # Install the CLI locally
	cargo build --release --bin goto
	mv target/release/goto /usr/local/bin/
	goto --version

build:
	cargo build --release --target $(DIETPI_TARGET)

build-zerow:
	cargo build --release --target $(ZEROW_TARGET)

build-zerow-cross: # Easier to setup but slower than direct compilation
	cross build --release --target $(ZEROW_TARGET)

# No scp root access, so we first get our files in a temporary dir, then move them with sudo
# TODO: compress before sending
deploy:
	# Binary
	ssh $(HOST) -- mkdir -p $(TMP_FOLDER)
	scp target/$(TARGET)/release/goto-api $(HOST):$(TMP_FOLDER)/goto-api
	ssh $(HOST) -- mv $(TMP_FOLDER)/goto-api /usr/local/bin/goto-api
	ssh $(HOST) -- rm -r $(TMP_FOLDER)

	ssh $(HOST) -- chown root:root /usr/local/bin/goto-api
	ssh $(HOST) -- chmod 755 /usr/local/bin/goto-api

	# Frontend
	ssh $(HOST) -- mkdir -p /etc/goto/dist
	ssh $(HOST) -- rm -rf /etc/goto/dist/*
	scp -r front/dist $(HOST):/etc/goto/dist/
	
	# Systemd Service
	scp goto.service $(HOST):/etc/systemd/system/
	ssh $(HOST) -- systemctl daemon-reload
	ssh $(HOST) -- systemctl restart goto.service
	ssh $(HOST) -- journalctl -u goto.service

tarpaulin:
	docker run \
		--rm \
		-v $(PWD):/volume \
		--entrypoint cargo \
		--security-opt seccomp=unconfined \
		xd009642/tarpaulin \
		tarpaulin --exclude-files front/*

coverage:
	rm -rf coverage/
	RUSTFLAGS="-Z instrument-coverage" \
		LLVM_PROFILE_FILE="goto-%p-%m.profraw" \
		cargo +nightly test

	grcov . --binary-path ./target/debug/ -s . -t html --branch --ignore-not-existing --ignore "*cargo*" -o ./coverage/
	rm *.profraw
	open coverage/index.html
