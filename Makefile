PREFIX?=	/usr/local
BINDIR=		$(PREFIX)/bin
RCDIR=		$(PREFIX)/etc/rc.d
STAGEDIR?=	${.CURDIR}/stage
VERSION?=	0.1.0

all:
	cargo build --release

stage: all
	rm -rf $(STAGEDIR)
	mkdir -p $(STAGEDIR)$(BINDIR)
	mkdir -p $(STAGEDIR)$(RCDIR)
	install -m 755 target/release/prometheus-bhyve-exporter $(STAGEDIR)$(BINDIR)/prometheus-bhyve-exporter
	install -m 755 port/files/bhyve_exporter.in $(STAGEDIR)$(RCDIR)/bhyve_exporter

install: all
	install -m 755 target/release/prometheus-bhyve-exporter $(BINDIR)/prometheus-bhyve-exporter
	install -m 755 port/files/bhyve_exporter.in $(RCDIR)/bhyve_exporter

package: stage
	pkg create -M +MANIFEST -p plist -r $(STAGEDIR) -o .

clean:
	cargo clean
	rm -rf $(STAGEDIR)

.PHONY: all stage install package clean
