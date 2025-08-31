.PHONY: deploy

TARGET=armv7-unknown-linux-gnueabihf
ARM_BIN=target/$(TARGET)/release/egui-version
BIN=target/release/egui-version
PI=caption.local
DEBIAN=debian.local
T430=t430.local


$(ARM_BIN): src
	cross build --release --target $(TARGET)

$(BIN): src
	cargo build --release

deploy: $(ARM_BIN) wordlists config.toml
	scp -r $^ $(PI):~

deploy-debian: $(BIN) wordlists config.toml
	scp -r $^ $(DEBIAN):~/Desktop/

deploy-t430: $(BIN) wordlists config.toml
	scp -r $^ $(T430):~/Desktop/

deps:
	ssh $(PI) sudo apt-get update "&&" sudo apt-get install -y \
		libxcursor1 \
		libxkbcommon-x11-0 \
		xinit
