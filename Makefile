# FPV Rush — build, check and package.
#
#   make              this help
#   make package      the installers for the machine you are on (linux or macos)
#   make linux        .deb + AppImage           — on Linux (or the devcontainer)
#   make macos        .dmg                      — on macOS
#
# One target per platform rather than `make package OS=…`: Tauri does not
# cross-compile between macOS and Linux, so the platform is never a choice —
# it is the machine. Each target says where it runs and stops elsewhere.
# Installers land in dist/.

.DEFAULT_GOAL := help
.PHONY: help tools deps check test app cli icons package linux macos dist-clean clean on-linux on-macos

# Every path is relative to the repository root, never absolute: make splits
# target and prerequisite names on spaces, and the checkout may well live in a
# folder like "FPV Rush extractor". Hence also: run make from the root.
ifeq ($(wildcard fpv-rs/Cargo.toml),)
$(error run make from the repository root (or: make -C <repository> …))
endif

WORKSPACE := fpv-rs
FRONTEND  := fpv-rs/app
SHELL_APP := fpv-rs/crates/fpv-app
# Relative to SHELL_APP, where the Tauri CLI is run from.
TAURI     := ../../app/node_modules/.bin/tauri
DIST      := dist
BUNDLE    := $(WORKSPACE)/target/$(if $(TARGET),$(TARGET)/)release/bundle
HOST      := $(shell uname -s)
ARCH      := $(shell uname -m)
# Says node_modules was installed *on this platform*. The checkout is shared
# between the Mac and the Linux devcontainer, and node_modules holds native
# binaries (the Tauri CLI, rolldown, lightningcss) for the platform that ran
# npm: installed in the container, it has no darwin binding and the Mac build
# dies with "Cannot find native binding". A per-platform stamp makes the first
# make on the other side reinstall. (npm ci empties node_modules, stamps too.)
NPM_STAMP := $(FRONTEND)/node_modules/.installed-$(HOST)-$(ARCH)

# Optional Rust target triple, e.g. `make macos TARGET=universal-apple-darwin`
# (needs `rustup target add aarch64-apple-darwin x86_64-apple-darwin`).
TARGET ?=
TARGET_FLAG := $(if $(TARGET),--target $(TARGET))
comma := ,

# cargo from rustup when it is not on PATH yet (fresh shells in the container).
export PATH := $(HOME)/.cargo/bin:$(PATH)

help: ## this help
	@echo "FPV Rush — make targets"
	@grep -E '^[a-z-]+:.*## ' Makefile | awk -F ':.*## ' '{ printf "  %-12s %s\n", $$1, $$2 }'
	@echo
	@echo "This machine: $(HOST). Installers go to dist/."

tools: ## check that the build tools are installed, and say how to get the missing ones
	@missing=0; \
	if ! command -v cargo >/dev/null 2>&1; then \
	  echo "missing: cargo (Rust) — $(if $(filter Darwin,$(HOST)),brew install rust$(comma) or rustup:,install it with rustup:)"; \
	  echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"; \
	  echo "  then open a new terminal (or: . \"\$$HOME/.cargo/env\")"; missing=1; fi; \
	if ! command -v npm >/dev/null 2>&1; then \
	  echo "missing: npm (Node.js) — $(if $(filter Darwin,$(HOST)),brew install node,sudo apt install nodejs npm)"; missing=1; fi; \
	if [ -n "$(TARGET)" ] && ! command -v rustup >/dev/null 2>&1; then \
	  echo "missing: rustup — TARGET=$(TARGET) needs extra Rust targets, which a Homebrew"; \
	  echo "  Rust cannot add. Install rustup (https://rustup.rs), then rustup target add …"; missing=1; fi; \
	if [ "$(HOST)" = Darwin ] && ! xcode-select -p >/dev/null 2>&1; then \
	  echo "missing: the Xcode command-line tools — xcode-select --install"; missing=1; fi; \
	if ! command -v ffmpeg >/dev/null 2>&1 && ! [ -x /opt/homebrew/bin/ffmpeg ] && ! [ -x /usr/local/bin/ffmpeg ]; then \
	  echo "note: no ffmpeg — the app builds, but needs it to run: $(if $(filter Darwin,$(HOST)),brew install ffmpeg,sudo apt install ffmpeg)"; fi; \
	exit $$missing

deps: tools $(NPM_STAMP) ## install the front end's npm packages

$(NPM_STAMP): $(FRONTEND)/package.json $(FRONTEND)/package-lock.json
	cd $(FRONTEND) && npm ci
	touch $@

check: deps ## clippy, every test (Rust and front end), and svelte-check — what must be green before packaging
	cd $(WORKSPACE) && cargo clippy --workspace --all-targets -- -D warnings
	cd $(WORKSPACE) && cargo test --workspace
	cd $(FRONTEND) && npm run -s test
	cd $(FRONTEND) && npm run -s check -- --fail-on-warnings

test: deps ## every test: the Rust crates and the front end's logic (Vitest)
	cd $(WORKSPACE) && cargo test --workspace
	cd $(FRONTEND) && npm run -s test

app: deps ## the app binary for this machine, no installer (fpv-rs/target/release/fpv-rush)
	cd $(SHELL_APP) && $(TAURI) build --no-bundle $(TARGET_FLAG)

icons: deps ## regenerate every icon size from fpv-rs/crates/fpv-app/icons/source.png (1024 px)
	cd $(SHELL_APP) && $(TAURI) icon icons/source.png -o icons
	rm -rf $(SHELL_APP)/icons/android $(SHELL_APP)/icons/ios

cli: tools ## the fpvlib command-line tool (fpv-rs/target/release/fpvlib)
	cd $(WORKSPACE) && cargo build --release -p fpvlib $(TARGET_FLAG)

package: ## the installers for this machine
ifeq ($(HOST),Linux)
	@$(MAKE) --no-print-directory linux
else ifeq ($(HOST),Darwin)
	@$(MAKE) --no-print-directory macos
else
	$(error no installer for $(HOST): build on Linux or macOS)
endif

# Platform guards, listed first so they stop the build before anything runs.
on-linux:
ifneq ($(HOST),Linux)
	$(error `make linux` runs on Linux — on a Mac, run it inside the devcontainer)
endif

on-macos:
ifneq ($(HOST),Darwin)
	$(error `make macos` runs on macOS — Tauri cannot build a Mac app from $(HOST))
endif

linux: on-linux deps ## .deb and AppImage (Linux only)
	cd $(SHELL_APP) && $(TAURI) build --bundles deb,appimage $(TARGET_FLAG)
	@mkdir -p $(DIST)
	cp $(BUNDLE)/deb/*.deb $(BUNDLE)/appimage/*.AppImage $(DIST)/
	@echo
	@echo "Linux installers in dist/:"
	@ls -1 $(DIST)/*.deb $(DIST)/*.AppImage
	@echo "The .deb pulls in ffmpeg and the GStreamer plugins; the AppImage expects them installed."

macos: on-macos deps ## a .dmg holding FPV Rush.app (macOS only)
	cd $(SHELL_APP) && $(TAURI) build --bundles app,dmg $(TARGET_FLAG)
	@mkdir -p $(DIST)
	cp $(BUNDLE)/dmg/*.dmg $(DIST)/
	@# Only the .dmg leaves: every FPV Rush.app on disk shows up in Launchpad and
	@# Spotlight, so neither dist/ nor Tauri's intermediate bundle keeps one —
	@# the copy dragged into /Applications is the app. (Earlier builds left both.)
	rm -rf "$(BUNDLE)/macos/FPV Rush.app" "$(DIST)/FPV Rush.app"
	@echo
	@echo "macOS installer in dist/ (open it, drag FPV Rush to Applications):"
	@ls -1 $(DIST)/*.dmg
	@echo "ffmpeg comes from Homebrew (brew install ffmpeg). Unsigned: first launch needs"
	@echo "right-click > Open, or: xattr -dr com.apple.quarantine \"/Applications/FPV Rush.app\""

dist-clean: ## empty dist/
	rm -rf $(DIST)

clean: dist-clean ## remove every build product (Rust target/, front-end build, dist/)
	cd $(WORKSPACE) && cargo clean
	rm -rf $(FRONTEND)/dist
