DIETPI_HOST ?= dietpi.local
DIETPI_TARGET ?= aarch64-unknown-linux-gnu

PIZEROW_HOST ?= pizero.local
ZEROW_TARGET ?= arm-unknown-linux-musleabi

HOST ?= $(DIETPI_HOST)
TARGET ?= $(DIETPI_TARGET)
TMP_FOLDER ?= /tmp/goto
DEPLOY_ARCHIVE ?= target/goto-deploy.tar.gz
DEPLOY_ARCHIVE_NAME := $(notdir $(DEPLOY_ARCHIVE)) # strips the folder, keeps the filename

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
	@dest="$$(command -v goto 2>/dev/null || echo $$HOME/.local/bin/goto)"; \
	echo "Installing to $$dest"; \
	mkdir -p "$$(dirname "$$dest")"; \
	cp target/release/goto "$$dest"
	goto --version

build:
	cargo build --release --target $(DIETPI_TARGET)

build-zerow:
	cargo build --release --target $(ZEROW_TARGET)

build-zerow-cross: # Easier to setup but slower than direct compilation
	cross build --release --target $(ZEROW_TARGET)

# Package everything into a single archive, copy it over, then unpack/move remotely.
# Deploying is particularly slow otherwise - also we can't directly scp the binary to
# its final destination.
deploy:
	@mkdir -p $(dir $(DEPLOY_ARCHIVE))
	rm -f $(DEPLOY_ARCHIVE)
	tar -czf $(DEPLOY_ARCHIVE) \
		-C target/$(TARGET)/release goto-api \
		-C $(CURDIR)/front dist \
		-C $(CURDIR) goto.service

	ssh $(HOST) -- mkdir -p $(TMP_FOLDER)
	scp $(DEPLOY_ARCHIVE) $(HOST):$(TMP_FOLDER)/$(DEPLOY_ARCHIVE_NAME)
	ssh $(HOST) -- tar -xzf $(TMP_FOLDER)/$(DEPLOY_ARCHIVE_NAME) -C $(TMP_FOLDER)

	# Binary
	ssh $(HOST) -- mv $(TMP_FOLDER)/goto-api /usr/local/bin/goto-api
	ssh $(HOST) -- chown root:root /usr/local/bin/goto-api
	ssh $(HOST) -- chmod 755 /usr/local/bin/goto-api

	# Frontend
	ssh $(HOST) -- mkdir -p /etc/goto
	ssh $(HOST) -- rm -rf /etc/goto/dist
	ssh $(HOST) -- mv $(TMP_FOLDER)/dist /etc/goto/dist

	# Systemd Service
	ssh $(HOST) -- mv $(TMP_FOLDER)/goto.service /etc/systemd/system/goto.service
	ssh $(HOST) -- systemctl daemon-reload
	ssh $(HOST) -- systemctl restart goto.service
	ssh $(HOST) -- journalctl -u goto.service

	ssh $(HOST) -- rm -rf $(TMP_FOLDER)

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
