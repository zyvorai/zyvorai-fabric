# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
.PHONY: build install tidy test

PROVIDER_NAME := terraform-provider-zyvor-fabricd
VERSION ?= 0.1.0

build:
	go build -o $(PROVIDER_NAME) .

tidy:
	go mod tidy

install: build
	mkdir -p ~/.terraform.d/plugins/registry.terraform.io/zyvorai/zyvor-fabricd/$(VERSION)/$$(go env GOOS)_$$(go env GOARCH)
	cp $(PROVIDER_NAME) ~/.terraform.d/plugins/registry.terraform.io/zyvorai/zyvor-fabricd/$(VERSION)/$$(go env GOOS)_$$(go env GOARCH)/$(PROVIDER_NAME)_v$(VERSION)

test:
	go test ./...
