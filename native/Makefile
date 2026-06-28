.PHONY: run release test clean desktop android windows setup-android dev-watch

CRATE := ha-game
PROFILE := --profile release-native

run:
	cargo run -p $(CRATE)

dev-watch:
	cargo watch -q -w crates -x 'run -p $(CRATE)'

release:
	cargo build -p $(CRATE) $(PROFILE)
	@echo "→ target/release-native/ha-game"

test:
	cargo test -p ha-shared
	cargo test -p $(CRATE) 2>/dev/null || true

clean:
	cargo clean

desktop:
	./scripts/build-desktop.sh

android: setup-android
	./scripts/build-android.sh

windows:
	rustup target add x86_64-pc-windows-gnu
	cargo build -p $(CRATE) --release --target x86_64-pc-windows-gnu

setup-android:
	./scripts/setup-android.sh
