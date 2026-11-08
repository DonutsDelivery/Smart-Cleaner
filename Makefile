PREFIX ?= $(HOME)/.local

.PHONY: build install run clean

build:
	cargo build --release

install: build
	-pkill -f sysclean 2>/dev/null || true
	install -Dm755 target/release/sysclean "$(PREFIX)/bin/sysclean"
	install -Dm644 icons/sysclean.svg "$(PREFIX)/share/icons/hicolor/scalable/apps/sysclean.svg"
	install -d "$(PREFIX)/share/applications"
	sed 's|^Exec=.*|Exec=$(PREFIX)/bin/sysclean|' data/sysclean.desktop > "$(PREFIX)/share/applications/sysclean.desktop"
	update-desktop-database "$(PREFIX)/share/applications" 2>/dev/null || true
	gtk4-update-icon-cache -f "$(PREFIX)/share/icons/hicolor" 2>/dev/null || true

run: install
	nohup $(PREFIX)/bin/sysclean &>/dev/null &

clean:
	cargo clean
