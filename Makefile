PREFIX?=	/usr/local
BINDIR=		$(PREFIX)/bin
RCDIR=		$(PREFIX)/etc/rc.d

all:
	cargo build --release

install: all
	install -m 755 target/release/prometheus-bhyve-exporter $(BINDIR)/prometheus-bhyve-exporter
	install -m 755 port/files/bhyve_exporter.in $(RCDIR)/bhyve_exporter

clean:
	cargo clean

.PHONY: all install clean
