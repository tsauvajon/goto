build-cross: # todo: compress before sending
	cross build --release --target arm-unknown-linux-musleabi
	scp target/arm-unknown-linux-musleabi/release/shorturl pi:/home/pi/shorturl
	scp -r front/dist pi:/home/pi/shorturl-dist

	ssh pi -- sudo mv /home/pi/shorturl /usr/local/bin/shorturl
	ssh pi -- sudo mv /home/pi/shorturl-dist /etc/shorturl/dist
	ssh pi -- sudo chown root:root /usr/local/bin/shorturl
	ssh pi -- sudo chmod 755 /usr/local/bin/shorturl
	ssh pi -- sudo systemctl restart shorturl.service
