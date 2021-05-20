build:
	wasm-pack build --target web --out-name wasm --out-dir ./dist
	cp static/* dist/

serve:
	miniserve ./dist --index index.html
