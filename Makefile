PREFIX ?= $(HOME)/.local
DESTDIR ?=
APP_ID = online.donutsdelivery.DonutCleaner
POLKIT_DIR ?= /usr/share/polkit-1/actions
# Polkit policies must live under /usr, so installing one needs root.
SUDO ?= $(if $(DESTDIR),,$(if $(filter 0,$(shell id -u)),,sudo))
SIZES = 16 24 32 48 64 128 256 512

.PHONY: build install install-policy run clean

build:
	cargo build --release

install: build install-policy
	-pkill -x donutcleaner 2>/dev/null || true
	install -Dm755 target/release/donutcleaner "$(DESTDIR)$(PREFIX)/bin/donutcleaner"
	install -Dm644 icons/$(APP_ID).svg "$(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/$(APP_ID).svg"
	for s in $(SIZES); do \
		install -Dm644 icons/hicolor/$${s}x$${s}/apps/$(APP_ID).png \
			"$(DESTDIR)$(PREFIX)/share/icons/hicolor/$${s}x$${s}/apps/$(APP_ID).png"; \
	done
	install -Dm644 data/$(APP_ID).desktop "$(DESTDIR)$(PREFIX)/share/applications/$(APP_ID).desktop"
	install -Dm644 data/$(APP_ID).metainfo.xml "$(DESTDIR)$(PREFIX)/share/metainfo/$(APP_ID).metainfo.xml"
	install -Dm644 LICENSE "$(DESTDIR)$(PREFIX)/share/licenses/donutcleaner/LICENSE"
	# Remove files left by the old SysClean name
	-rm -f "$(DESTDIR)$(PREFIX)/bin/sysclean" \
		"$(DESTDIR)$(PREFIX)/share/applications/sysclean.desktop" \
		"$(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/sysclean.svg"
	-gtk4-update-icon-cache -f "$(DESTDIR)$(PREFIX)/share/icons/hicolor" 2>/dev/null || true

install-policy:
	$(SUDO) install -Dm644 data/$(APP_ID).policy "$(DESTDIR)$(POLKIT_DIR)/$(APP_ID).policy"

run: install
	nohup "$(PREFIX)/bin/donutcleaner" &>/dev/null &

clean:
	cargo clean
