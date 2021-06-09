build:
	build --release --target arm-unknown-linux-musleabi
	$(MAKE) deploy

build-cross: # todo: compress before sending
	cross build --release --target arm-unknown-linux-musleabi
	$(MAKE) deploy

deploy:
	scp target/arm-unknown-linux-musleabi/release/shorturl pi:/home/pi/shorturl
	scp -r front/dist pi:/home/pi/shorturl-dist

	ssh pi -- sudo mv /home/pi/shorturl /usr/local/bin/shorturl
	ssh pi -- sudo mkdir -p /etc/shorturl/dist
	ssh pi -- sudo rm -rf /etc/shorturl/dist/*
	ssh pi -- sudo mv /home/pi/shorturl-dist/* /etc/shorturl/dist/*
	ssh pi -- sudo rm -r /home/pi/shorturl-dist
	ssh pi -- sudo chown root:root /usr/local/bin/shorturl
	ssh pi -- sudo chmod 755 /usr/local/bin/shorturl
	ssh pi -- sudo systemctl restart shorturl.service
	ssh pi -- sudo journalctl -u shorturl.service

tarpaulin:
	docker run \
		--rm \
		-v $(PWD):/volume \
		--entrypoint cargo \
		--security-opt seccomp=unconfined \
		xd009642/tarpaulin \
		tarpaulin --exclude-files front/*
