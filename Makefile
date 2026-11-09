PREFIX ?= $(HOME)/.local
DESTDIR ?=
APP_ID = online.donutsdelivery.DonutCleaner
# The polkit policy names the helper by absolute path, so neither follows PREFIX.
HELPER_DIR ?= /usr/lib/donutcleaner
POLKIT_DIR ?= /usr/share/polkit-1/actions
# Both paths belong to root. Without DESTDIR and outside a root shell, use sudo.
SUDO ?= $(if $(DESTDIR),,$(if $(filter 0,$(shell id -u)),,sudo))
SIZES = 16 24 32 48 64 128 256 512

.PHONY: build install install-helper run clean

build:
	cargo build --release

install: build install-helper
	-pkill -x donutcleaner 2>/dev/null || true
	install -Dm755 target/release/donutcleaner "$(DESTDIR)$(PREFIX)/bin/donutcleaner"
	install -Dm644 icons/$(APP_ID).svg "$(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/$(APP_ID).svg"
	for s in $(SIZES); do \
		install -Dm644 icons/hicolor/$${s}x$${s}/apps/$(APP_ID).png \
			"$(DESTDIR)$(PREFIX)/share/icons/hicolor/$${s}x$${s}/apps/$(APP_ID).png"; \
	done
	install -d "$(DESTDIR)$(PREFIX)/share/applications"
	sed 's|^Exec=.*|Exec=$(PREFIX)/bin/donutcleaner|' data/$(APP_ID).desktop > "$(DESTDIR)$(PREFIX)/share/applications/$(APP_ID).desktop"
	install -Dm644 data/$(APP_ID).metainfo.xml "$(DESTDIR)$(PREFIX)/share/metainfo/$(APP_ID).metainfo.xml"
	install -Dm644 LICENSE "$(DESTDIR)$(PREFIX)/share/licenses/donutcleaner/LICENSE"
	# Remove files left by the old SysClean name
	-rm -f "$(DESTDIR)$(PREFIX)/bin/sysclean" \
		"$(DESTDIR)$(PREFIX)/share/applications/sysclean.desktop" \
		"$(DESTDIR)$(PREFIX)/share/icons/hicolor/scalable/apps/sysclean.svg"
	-gtk4-update-icon-cache -f "$(DESTDIR)$(PREFIX)/share/icons/hicolor" 2>/dev/null || true
	-update-desktop-database "$(DESTDIR)$(PREFIX)/share/applications" 2>/dev/null || true

install-helper: build
	$(SUDO) install -Dm755 target/release/donutcleaner-helper "$(DESTDIR)$(HELPER_DIR)/donutcleaner-helper"
	$(SUDO) install -Dm644 data/$(APP_ID).helper.policy "$(DESTDIR)$(POLKIT_DIR)/$(APP_ID).helper.policy"

run: install
	nohup "$(PREFIX)/bin/donutcleaner" &>/dev/null &

clean:
	cargo clean
