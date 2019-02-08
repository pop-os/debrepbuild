prefix ?= /usr/local
DEBUG ?= 0
PACKAGE=debrep
TARGET = debug

ifeq (0,$(DEBUG))
        ARGS += --release
        TARGET = release
endif

BINARY=target/$(TARGET)/$(PACKAGE)

all: $(BINARY)

clean:
	cargo clean

distclean: clean
	rm -rf .cargo vendor

install:
	install -Dm0755 $(BINARY) $(DESTDIR)$(prefix)/bin/$(PACKAGE)

.cargo/config: vendor_config
	mkdir -p .cargo
	cp $< $@

vendor: .cargo/config
	cargo vendor
	touch vendor

$(BINARY): Cargo.lock Cargo.toml src/*.rs src/**/**/*.rs
	cargo build $(ARGS)
