# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: Apache-2.0
#
# Solvor, the Mac app. Run `make help`. macOS with Xcode only; nothing here signs, notarizes or uploads anything.
.DEFAULT_GOAL := help
.PHONY: help project build test test-kit test-app run stop open catalog icon export verify-page dmg release clean

APP      := Solvor
PROJECT  := $(APP).xcodeproj
DERIVED  ?= build/DerivedData
DEST     := platform=macOS
APP_PATH := $(DERIVED)/Build/Products/Debug/$(APP).app
XCB      := xcodebuild -project $(PROJECT) -scheme $(APP) -destination '$(DEST)' -derivedDataPath $(DERIVED)

help: ## list the targets
	@grep -E '^[a-z-]+:.*## ' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  make %-10s %s\n", $$1, $$2}'
	@printf '\n  make run KEEP_HOST=http://127.0.0.1:9096 KEEP_TOKEN=...   debug build, token from the environment (no Keychain prompt)\n'

project: ## regenerate Solvor.xcodeproj from project.yml (needs: brew install xcodegen)
	@command -v xcodegen >/dev/null || { echo "xcodegen is not installed: brew install xcodegen" >&2; exit 1; }
	xcodegen generate

build: ## build the app (Debug, ad-hoc signed) into build/DerivedData
	$(XCB) build

test: test-kit test-app ## MerionKit unit tests, then the app's tests

test-kit: ## swift test (the live tests skip unless KEEP_API and KEEP_TOKEN are set)
	swift test

test-app: ## xcodebuild test for the app target
	$(XCB) CODE_SIGNING_ALLOWED=NO test

run: build ## build and open the app; KEEP_HOST and KEEP_TOKEN, if both set, log a debug build in without the Keychain
	@pkill -x $(APP) 2>/dev/null || true
	@if [ -n "$(KEEP_HOST)" ] && [ -n "$(KEEP_TOKEN)" ]; then \
	  open --env KEEP_DEV_HOST='$(KEEP_HOST)' --env KEEP_DEV_TOKEN='$(KEEP_TOKEN)' '$(APP_PATH)'; \
	else open '$(APP_PATH)'; fi

stop: ## quit the app
	@pkill -x $(APP) 2>/dev/null || true

open: ## open the project in Xcode
	open $(PROJECT)

catalog: ## regenerate Sources/MerionKit/CatalogData.swift from examples/merion-agents
	python3 tools/gen_catalog.py

icon: ## regenerate the app icon and menu-bar glyph (needs rsvg-convert)
	python3 tools/make_icon.py

export: ## copy this folder into a checkout of the standalone repo: make export DEST=../solvor
	@test -n "$(DEST)" || { echo 'usage: make export DEST=path/to/solvor-checkout' >&2; exit 1; }
	tools/export.sh '$(DEST)'

verify-page: ## serve the local test webmail page on http://127.0.0.1:8765/webmail-test.html (see docs/VERIFY.md)
	@echo 'open http://127.0.0.1:8765/webmail-test.html in your browser; Ctrl-C to stop'
	python3 -m http.server 8765 --bind 127.0.0.1 --directory docs/verify

dmg: ## Release build and a .dmg, ad-hoc signed: runs on this Mac only (see docs/RELEASING.md for signing and notarizing)
	tools/release.sh

release: ## signed, notarized dmg: make release SIGN_IDENTITY="Developer ID Application: ..." NOTARY_PROFILE=name
	tools/release.sh --sign --notarize

clean: ## remove build products
	rm -rf build .build
