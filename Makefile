build:
	cargo build --release --target arm-unknown-linux-musleabi
	$(MAKE) deploy

build-cli:
	cargo build --release --bin goto
	mv target/release/goto /usr/local/bin/
	goto --version

build-cross: # todo: compress before sending
	cross build --release --target arm-unknown-linux-musleabi
	scp target/arm-unknown-linux-musleabi/release/goto-api pi:/home/pi/goto-api
	scp -r front/dist pi:/home/pi/goto-dist

	ssh pi -- sudo mv /home/pi/goto-api /usr/local/bin/goto-api
	ssh pi -- sudo mkdir -p /etc/goto/dist
	ssh pi -- sudo rm -rf /etc/goto/dist/*
	ssh pi -- sudo mv /home/pi/goto-dist/* /etc/goto/dist/*
	ssh pi -- sudo rm -r /home/pi/goto-dist
	ssh pi -- sudo chown root:root /usr/local/bin/goto
	ssh pi -- sudo chmod 755 /usr/local/bin/goto
	ssh pi -- sudo systemctl restart goto.service
	ssh pi -- sudo journalctl -u goto.service

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
