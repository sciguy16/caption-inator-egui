.PHONY: deploy

TARGET=armv7-unknown-linux-gnueabihf
BIN=target/$(TARGET)/release/egui-version
PI=caption.local

$(BIN): src
	cross build --release --target $(TARGET)

deploy: $(BIN) wordlists config.toml
	scp -r $^ $(PI):~

deps:
	ssh $(PI) sudo apt-get update "&&" sudo apt-get install -y \
		libxcursor1 \
		libxkbcommon-x11-0 \
		xinit
