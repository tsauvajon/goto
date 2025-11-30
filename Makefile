build: # RPi Zero 2W (DietPi)
	cargo build --release --target aarch64-unknown-linux-gnu
	$(MAKE) deploy

build-zerow: # RPi Zero W
	cargo build --release --target arm-unknown-linux-musleabi
	$(MAKE) deploy-zerow

build-cli:
	cargo build --release --bin goto
	mv target/release/goto /usr/local/bin/
	goto --version

build-cross: # todo: compress before sending
	cross build --release --target arm-unknown-linux-musleabi
	$(MAKE) deploy

deploy-zerow:
	# No scp root access, so we first get our files in our user's home, then move them with sudo
	scp target/aarch64-unknown-linux-gnu/release/goto-api dietpi.local:/home/dietpi/goto-api
	scp -r front/dist dietpi.local:/home/dietpi/goto-dist
	scp goto.service dietpi.local:/home/dietpi/goto.service

	ssh dietpi.local -- sudo mv /home/dietpi/goto-api /usr/local/bin/goto-api
	ssh dietpi.local -- sudo mkdir -p /etc/goto/dist
	ssh dietpi.local -- sudo rm -rf /etc/goto/dist/*
	ssh dietpi.local -- sudo mv /home/dietpi/goto-dist/* /etc/goto/dist/
	ssh dietpi.local -- sudo rm -r /home/dietpi/goto-dist
	ssh dietpi.local -- sudo chown root:root /usr/local/bin/goto-api
	ssh dietpi.local -- sudo chmod 755 /usr/local/bin/goto-api
	ssh dietpi.local -- sudo mv /home/dietpi/goto.service /etc/systemd/system/goto.service
	ssh dietpi.local -- sudo systemctl restart goto.service
	ssh dietpi.local -- sudo journalctl -u goto.service

deploy:
	# Binary
	scp target/aarch64-unknown-linux-gnu/release/goto-api dietpi.local:/usr/local/bin/goto-api
	ssh dietpi.local -- chown root:root /usr/local/bin/goto-api
	ssh dietpi.local -- chmod 755 /usr/local/bin/goto-api

	# Frontend
	ssh dietpi.local -- mkdir -p /etc/goto/dist
	ssh dietpi.local -- rm -rf /etc/goto/dist/*
	scp -r front/dist dietpi.local:/etc/goto/dist/
	
	# Service
	scp goto.service dietpi.local:/etc/systemd/system/
	ssh dietpi.local -- systemctl daemon-reload
	ssh dietpi.local -- systemctl restart goto.service
	ssh dietpi.local -- journalctl -u goto.service

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
