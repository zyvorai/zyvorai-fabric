# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
.PHONY: all build build-backend build-web \
       install install-bin install-conf install-systemd install-web install-modules install-libexec \
       uninstall run dev cli test clean fmt lint \
       doctor doctor-check \
       docker-build docker-build-fluxvm docker-up docker-down \
       k8s-deploy k8s-undeploy helm-lint helm-template \
       rpm deb help

PREFIX     ?= /usr
DESTDIR    ?=
BINDIR      = $(PREFIX)/bin
SYSCONFDIR  = /etc
DATADIR     = $(PREFIX)/share
# systemd is optional (see systemd/zyvor-fabricd.service) — nothing under
# install-* requires, auto-enables, or auto-starts it. install-systemd only
# drops the unit files in place for operators who choose to run under it.
UNITDIR     = $(PREFIX)/lib/systemd/system
MODULESDIR  = $(SYSCONFDIR)/modules-load.d
LIBEXECDIR  = $(PREFIX)/libexec/zyvor-fabricd

all: build

build: build-backend build-web

build-backend:
	cd backend && cargo build --release

build-web:
	cd web && npm install && npm run build

install: install-bin install-conf install-systemd install-web install-modules install-libexec

install-bin:
	install -d $(DESTDIR)$(BINDIR)
	install -m 0755 backend/target/release/zyvor-fabricd  $(DESTDIR)$(BINDIR)/zyvor-fabricd
	install -m 0755 backend/target/release/fabricctl      $(DESTDIR)$(BINDIR)/fabricctl
	# fabricctl was named zyvorctl before the rename; drop a stale copy
	rm -f  $(DESTDIR)$(BINDIR)/zyvorctl

# Directories are also created defensively by the daemon itself at startup
# (see daemon.rs::ensure_runtime_dirs) — installed here too so they exist
# with the right ownership/mode from first boot, without depending on
# systemd-tmpfiles.
install-conf:
	install -d $(DESTDIR)$(SYSCONFDIR)/zyvor-fabricd
	install -m 0644 configs/zyvor-fabricd.toml $(DESTDIR)$(SYSCONFDIR)/zyvor-fabricd/zyvor-fabricd.toml
	install -m 0644 configs/zyvor-fabricd.env  $(DESTDIR)$(SYSCONFDIR)/zyvor-fabricd/zyvor-fabricd.env
	install -d $(DESTDIR)/var/lib/zyvor-fabricd/images
	install -d -m 0750 $(DESTDIR)/var/lib/zyvor-fabricd/state
	install -d $(DESTDIR)/var/log/zyvor-fabricd
	install -d $(DESTDIR)/run/zyvor-fabricd

# Optional: unit files for operators who choose to run zyvor-fabricd under
# systemd. Installing them here does not enable, start, or otherwise wire
# them up — that's a manual `systemctl enable --now zyvor-fabricd.service`.
install-systemd:
	install -d $(DESTDIR)$(UNITDIR)
	install -m 0644 systemd/zyvor-fabricd.service $(DESTDIR)$(UNITDIR)/zyvor-fabricd.service

install-libexec:
	install -d $(DESTDIR)$(LIBEXECDIR)
	install -m 0755 scripts/backup-vms    $(DESTDIR)$(LIBEXECDIR)/backup-vms
	install -m 0755 scripts/cleanup-store $(DESTDIR)$(LIBEXECDIR)/cleanup-store
	install -m 0755 scripts/health-check  $(DESTDIR)$(LIBEXECDIR)/health-check

install-web:
	install -d $(DESTDIR)$(DATADIR)/zyvor-fabricd/web
	cp -r web/dist/* $(DESTDIR)$(DATADIR)/zyvor-fabricd/web/

install-modules:
	install -d $(DESTDIR)$(MODULESDIR)
	install -m 0644 configs/modules-load.d/zyvor-fabricd.conf $(DESTDIR)$(MODULESDIR)/zyvor-fabricd.conf
	install -d $(DESTDIR)/etc/logrotate.d
	install -m 0644 configs/logrotate.d/zyvor-fabricd $(DESTDIR)/etc/logrotate.d/zyvor-fabricd
	install -d $(DESTDIR)/etc/bash_completion.d
	install -m 0644 completions/fabricctl.bash $(DESTDIR)/etc/bash_completion.d/fabricctl
	rm -f  $(DESTDIR)/etc/bash_completion.d/zyvorctl

uninstall:
	rm -f  $(DESTDIR)$(BINDIR)/zyvor-fabricd
	rm -f  $(DESTDIR)$(BINDIR)/fabricctl $(DESTDIR)$(BINDIR)/zyvorctl
	rm -f  $(DESTDIR)$(UNITDIR)/zyvor-fabricd.service
	rm -f  $(DESTDIR)$(UNITDIR)/vm@.service
	rm -f  $(DESTDIR)/etc/logrotate.d/zyvor-fabricd
	rm -f  $(DESTDIR)/etc/bash_completion.d/fabricctl $(DESTDIR)/etc/bash_completion.d/zyvorctl
	rm -rf $(DESTDIR)$(LIBEXECDIR)
	rm -rf $(DESTDIR)$(DATADIR)/zyvor-fabricd
	rm -rf $(DESTDIR)$(SYSCONFDIR)/zyvor-fabricd

run:
	cd backend && ZYVOR_FABRICD_LOG_LEVEL=info cargo run --bin zyvor-fabricd

run-debug:
	cd backend && ZYVOR_FABRICD_LOG_LEVEL=debug cargo run --bin zyvor-fabricd

dev:
	@echo "Starting Zyvor Fabric (zyvor-fabricd) in development mode..."
	@cd backend && ZYVOR_FABRICD_LOG_LEVEL=debug cargo run --bin zyvor-fabricd &
	@cd web && npm run dev

cli:
	cd backend && cargo run --bin fabricctl

test:
	cd backend && cargo test
	cd web && npm test
	$(MAKE) test-proven-infra
	$(MAKE) test-devops

.PHONY: test-proven-infra bench
test-proven-infra:
	python3 -m unittest benchmarks.test_harness -v
	bash scripts/test-upgrade-rollback.sh
	bash scripts/chaos-qualify.sh

.PHONY: test-devops
test-devops:
	cd examples/devops && python3 -m unittest test_contract.py test_examples.py -v
	bash scripts/test-devops-gate.sh

bench:
	python3 benchmarks/harness.py --allow-offline --out /tmp/fabric-bench-offline.json

doctor:
	$(MAKE) -C tools/fabric-doctor build

doctor-check:
	$(MAKE) -C tools/fabric-doctor check

clean:
	cd backend && cargo clean
	cd web && rm -rf node_modules dist

fmt:
	cd backend && cargo fmt

lint:
	cd backend && cargo clippy

rpm:
	rpmbuild -ba zyvor-fabricd.spec --define "_topdir $(PWD)/rpmbuild" --define "_sourcedir $(PWD)"

deb:
	dpkg-buildpackage -us -uc -b

docker-build: docker-build-fluxvm
	docker compose build

docker-build-fluxvm:
	./scripts/build-container-images.sh

docker-up:
	docker compose up -d

docker-down:
	docker compose down

k8s-deploy:
	./scripts/deploy-k8s.sh

k8s-undeploy:
	kubectl delete namespace zyvor-fabric --wait=false || true

helm-lint:
	helm lint ./charts/zyvor-fabric

helm-template:
	helm template zyvor-fabric ./charts/zyvor-fabric \
		--namespace zyvor-fabric \
		--set security.adminUsername=admin \
		--set security.adminPassword=eval-admin-only \
		--set security.jwtSecret=eval-jwt-secret-at-least-32-chars

help:
	@echo "Available targets:"
	@echo "  build           - Build backend and web UI"
	@echo "  install         - Install everything (use DESTDIR= for staged installs)"
	@echo "  install-bin     - Install binaries only"
	@echo "  install-conf    - Install configuration files"
	@echo "  install-systemd - Install optional systemd unit files (not enabled/started)"
	@echo "  install-libexec - Install backup/cleanup/health-check scripts"
	@echo "  install-web     - Install web UI static files"
	@echo "  install-modules - Install kernel module config"
	@echo "  uninstall       - Remove installed files"
	@echo "  run             - Run daemon (info log level)"
	@echo "  run-debug       - Run daemon (debug log level)"
	@echo "  dev             - Run in development mode (debug + web dev server)"
	@echo "  cli             - Run CLI"
	@echo "  test            - Run tests"
	@echo "  doctor          - Build fabric-doctor (Go preflight tool)"
	@echo "  doctor-check    - Format/vet/test/build fabric-doctor"
	@echo "  clean           - Clean build artifacts"
	@echo "  fmt             - Format code"
	@echo "  lint            - Lint code"
	@echo "  rpm             - Build RPM package"
	@echo "  deb             - Build Debian package"
	@echo "  docker-build    - Build zyvor-fabricd and fluxvm images (see docs/DOCKER.md)"
	@echo "  docker-up       - Start with Docker/Podman Compose"
	@echo "  docker-down     - Stop Docker/Podman Compose"
	@echo "  k8s-deploy      - Apply k8s/base manifests (local kubectl)"
	@echo "  k8s-undeploy    - Delete zyvor-fabric namespace"
	@echo "  helm-lint       - Lint charts/zyvor-fabric"
	@echo "  helm-template   - Render Helm chart (eval secrets)"
	@echo ""
	@echo "Variables:"
	@echo "  PREFIX=$(PREFIX)  DESTDIR=$(DESTDIR)"
	@echo "  BINDIR=$(BINDIR)  SYSCONFDIR=$(SYSCONFDIR)"
	@echo "  UNITDIR=$(UNITDIR)"
