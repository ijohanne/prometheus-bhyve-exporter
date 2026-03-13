PORTNAME=	prometheus-bhyve-exporter
PORTVERSION=	0.1.0
CATEGORIES=	sysutils
MASTER_SITES=	# built from local source

MAINTAINER=	ij@unixpimps.net
COMMENT=	Prometheus exporter for FreeBSD bhyve VMs managed by vm-bhyve

LICENSE=	MIT

BUILD_DEPENDS=	cargo:lang/rust

USES=		cargo

USE_RC_SUBR=	bhyve_exporter

PLIST_FILES=	bin/prometheus-bhyve-exporter

do-install:
	${INSTALL_PROGRAM} ${CARGO_TARGET_DIR}/release/${PORTNAME} \
		${STAGEDIR}${PREFIX}/bin/${PORTNAME}

.include <bsd.port.mk>
