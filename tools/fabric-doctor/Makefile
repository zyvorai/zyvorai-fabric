# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0
BINARY ?= fabric-doctor
VERSION ?= dev
LDFLAGS := -s -w -X main.version=$(VERSION)

.PHONY: fmt vet test race build check clean

fmt:
	gofmt -w ./cmd ./internal

vet:
	go vet ./...

test:
	go test ./...

race:
	go test -race ./...

build:
	CGO_ENABLED=0 go build -trimpath -ldflags '$(LDFLAGS)' -o bin/$(BINARY) ./cmd/fabric-doctor

check:
	@test -z "$$(gofmt -l ./cmd ./internal)" || (echo "gofmt required"; gofmt -l ./cmd ./internal; exit 1)
	go vet ./...
	go test ./...
	CGO_ENABLED=0 go build -trimpath -ldflags '$(LDFLAGS)' -o bin/$(BINARY) ./cmd/fabric-doctor

clean:
	rm -rf bin
