PREFIX ?= /usr/local
PACKAGE=debrep
BINARY=target/release/$(PACKAGE)

all: $(BINARY)

clean:
	cargo clean

distclean: clean
	rm -rf .cargo vendor

install:
	install -Dm0755 $(BINARY) $(DESTDIR)$(PREFIX)/bin/$(PACKAGE)

.cargo/config: vendor_config
	mkdir -p .cargo
	cp $< $@

vendor: .cargo/config
	cargo vendor
	touch vendor

$(BINARY):
	cargo build --release
