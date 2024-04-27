PREFIX = /usr/local

all: build
build: 
	cargo build --release


clean:
	cargo clean

install:
	mkdir -p $(DESTDIR)/bin
	mkdir -p $(PREFIX)/share/biblia-cli-rs
	cp -f target/release/biblia-cli-rs $(DESTDIR)$(PREFIX)/bin
	chmod 755 $(DESTDIR)$(PREFIX)/bin/biblia-cli-rs
	cp -f src/biblia.json $(DESTDIR)$(PREFIX)/share/biblia-cli-rs/biblia.json
	@echo
	@echo "Am instalat biblia-cli-rs in $(DESTDIR)$(PREFIX)/bin";
uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/biblia-cli-rs

.PHONY: all build clean install uninstall
