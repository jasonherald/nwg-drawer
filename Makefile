# Makefile for nwg-drawer — binary-repo subset per epic §3.6.
#
# Default install target is /usr/local (LSB convention for locally-built
# software). Contributors iterating from a clone should use the no-sudo
# override:
#   make install PREFIX=$HOME/.local BINDIR=$HOME/.cargo/bin
# Go-predecessor parity is an opt-in:
#   sudo make install PREFIX=/usr

CARGO   ?= cargo
PREFIX  ?= /usr/local
BINDIR  ?= $(PREFIX)/bin
DATADIR ?= $(PREFIX)/share
DESTDIR ?=

BIN_NAME      := nwg-drawer
DATA_APP_NAME := nwg-drawer

SONAR_SCANNER ?= /opt/sonar-scanner/bin/sonar-scanner
SONAR_HOST_URL ?= https://sonar.aaru.network
SONAR_TRUSTSTORE ?= /tmp/sonar-truststore.jks
SONAR_TRUSTSTORE_PASSWORD ?= changeit

.PHONY: all build build-release test lint check-tools \
        lint-fmt lint-clippy lint-test lint-deny lint-audit \
        install install-bin install-data uninstall \
        upgrade \
        sonar clean help

all: build

define HELP_TEXT
Targets:
  make build           Debug build
  make build-release   Release build (used by install + upgrade)
  make test            cargo test + cargo clippy --all-targets
  make lint            Full local check: fmt + clippy + test + deny + audit
  make install         Build release + install binary + install data
  make install-bin     Install binary to $(DESTDIR)$(BINDIR)
  make install-data    Install data assets to $(DESTDIR)$(DATADIR)/$(DATA_APP_NAME)/
  make uninstall       Remove installed binary and data
  make upgrade         On-demand-aware: rebuild + install; if any drawer instance is running (resident or transient), stop + restart with captured args
  make sonar           Run SonarQube scan (requires sonar-scanner + .env)
  make clean           cargo clean

Install-path invocations:
  sudo make install                                              # default /usr/local
  make install PREFIX=$$HOME/.local BINDIR=$$HOME/.cargo/bin     # no-sudo dev
  sudo make install PREFIX=/usr                                  # distro-parity
endef
export HELP_TEXT

help:
	@echo "$$HELP_TEXT"

build:
	$(CARGO) build

build-release:
	$(CARGO) build --release

test:
	$(CARGO) test
	$(CARGO) clippy --all-targets

check-tools:
	@if ! command -v cargo-deny >/dev/null 2>&1; then \
		echo "Installing cargo-deny..."; \
		$(CARGO) install cargo-deny; \
	fi
	@if ! command -v cargo-audit >/dev/null 2>&1; then \
		echo "Installing cargo-audit..."; \
		$(CARGO) install cargo-audit; \
	fi

# Individual lint subtargets — each runnable on its own.
lint-fmt:
	@echo "── Format ──"
	$(CARGO) fmt --all --check

lint-clippy:
	@echo "── Clippy ──"
	$(CARGO) clippy --all-targets -- -D warnings

# Plain test (no clippy) so `make lint` runs clippy exactly once via lint-clippy.
lint-test:
	@echo "── Tests ──"
	$(CARGO) test

lint-deny:
	@echo "── Cargo Deny (licenses, advisories, bans, sources) ──"
	$(CARGO) deny check

lint-audit:
	@echo "── Cargo Audit (dependency CVEs) ──"
	$(CARGO) audit

lint: check-tools lint-fmt lint-clippy lint-test lint-deny lint-audit
	@echo ""
	@echo "All local checks passed ✓"

# ─────────────────────────────────────────────────────────────────────
# Install / uninstall
# ─────────────────────────────────────────────────────────────────────

install: build-release install-bin install-data

install-bin:
	@echo "Installing binary to $(DESTDIR)$(BINDIR)/$(BIN_NAME)"
	install -D -m 755 target/release/$(BIN_NAME) "$(DESTDIR)$(BINDIR)/$(BIN_NAME)"

install-data:
	@echo "Installing data assets to $(DESTDIR)$(DATADIR)/$(DATA_APP_NAME)/"
	install -d "$(DESTDIR)$(DATADIR)/$(DATA_APP_NAME)/img"
	install -m 644 data/$(DATA_APP_NAME)/drawer.css "$(DESTDIR)$(DATADIR)/$(DATA_APP_NAME)/"
	install -m 644 data/$(DATA_APP_NAME)/img/*.svg "$(DESTDIR)$(DATADIR)/$(DATA_APP_NAME)/img/"

uninstall:
	@echo "Removing binary + data"
	rm -f "$(DESTDIR)$(BINDIR)/$(BIN_NAME)"
	rm -rf "$(DESTDIR)$(DATADIR)/$(DATA_APP_NAME)"
	@echo "Uninstalled."

# ─────────────────────────────────────────────────────────────────────
# Upgrade — on-demand-aware.
# ─────────────────────────────────────────────────────────────────────
#
# The drawer is usually spawned per-click from the dock's launcher
# button, not resident. If no running instance is found we just
# rebuild + install — the user's next launch picks up the new binary.
# If ANY instance is running (whether resident `-r` or a still-open
# transient launch) we capture args via --dump-args, stop, install,
# and restart with the captured args.
#
# Linux-only: `pidof` comes from procps-ng and is GNU/Linux-specific.
# The drawer targets Hyprland + Sway (Linux Wayland compositors), so
# cross-platform Makefile support is out of scope — if that ever
# changes, a /proc-based or `pgrep`-based fallback goes here.
#
# Install-target validation (issue #24): before killing anything,
# resolve /proc/$PID/exe for each running drawer and compare against
# where this upgrade would install ($(BINDIR)/$(BIN_NAME)). If they
# don't match — usually because the user installed to ~/.cargo/bin
# but invoked upgrade without re-passing PREFIX/BINDIR, so we'd try
# to install to /usr/local and fail on permission — we abort with a
# helpful error BEFORE touching the drawer. Previously the recipe
# killed the drawer first and then failed the install, leaving the
# desktop session with no running drawer and no binary update.
#
# Atomicity: recipe order is validate → capture args → install →
# kill → restart. Install happens while the drawer is still running
# (Linux's mmap semantics mean replacing the binary file via
# `install`'s unlink+write doesn't disturb the running process's
# loaded pages). If install fails, the drawer is never killed.
#
# PID identity validation (CodeRabbit follow-up): we capture
# `/proc/$PID/stat` field 22 (starttime — clock ticks since boot)
# alongside each pid at discovery, and re-verify it matches before
# sending SIGTERM and SIGKILL. Starttime is kernel-authoritative
# and unique per (pid, boot), so a reused pid with a different
# process attached gets dropped from the kill list with a
# 'no longer our drawer' message rather than SIGKILLed blindly.
#
# --dump-args failure handling: a failure is only swallowed when
# the pid has actually disappeared (no `/proc/$PID/exe`). If
# --dump-args fails on a still-live drawer that's a real bug —
# fail-fast with an explicit error rather than silently killing
# the drawer without capturing its args.
upgrade: build-release
	@RUNNING_PIDS="$$(pidof -c $(BIN_NAME) 2>/dev/null || true)"; \
	if [ -n "$$RUNNING_PIDS" ]; then \
		INSTALL_TARGET="$(DESTDIR)$(BINDIR)/$(BIN_NAME)"; \
		INSTALL_TARGET_REAL="$$(readlink -f "$$INSTALL_TARGET" 2>/dev/null || echo "$$INSTALL_TARGET")"; \
		for pid in $$RUNNING_PIDS; do \
			RUNNING_EXE="$$(readlink -f "/proc/$$pid/exe" 2>/dev/null)"; \
			if [ -z "$$RUNNING_EXE" ]; then \
				if [ -d "/proc/$$pid" ]; then \
					echo "ERROR: unable to resolve /proc/$$pid/exe for live drawer pid $$pid"; \
					echo "       (process is alive but its exe symlink is unreadable — refusing to proceed"; \
					echo "        without install-target validation)"; \
					exit 1; \
				fi; \
				continue; \
			fi; \
			if [ "$$RUNNING_EXE" != "$$INSTALL_TARGET_REAL" ]; then \
				RUNNING_BINDIR="$$(dirname "$$RUNNING_EXE")"; \
				RUNNING_PREFIX="$$(dirname "$$RUNNING_BINDIR")"; \
				echo "ERROR: running drawer (pid $$pid) is installed at"; \
				echo "         $$RUNNING_EXE"; \
				echo "       but 'make upgrade' would install to"; \
				echo "         $$INSTALL_TARGET"; \
				echo ""; \
				echo "       Drawer NOT killed — a prefix-mismatched upgrade would leave"; \
				echo "       you with no running drawer and no new binary."; \
				echo ""; \
				echo "       Re-run with PREFIX/BINDIR matching the running binary:"; \
				echo "         make upgrade PREFIX=$$RUNNING_PREFIX BINDIR=$$RUNNING_BINDIR"; \
				echo "       (or stop the drawer manually and re-run make install)."; \
				exit 1; \
			fi; \
		done; \
		ARGS_FILE="$$(mktemp)" || exit 1; \
		RUNNING_INFO="$$(mktemp)" || exit 1; \
		trap 'rm -f "$$ARGS_FILE" "$$RUNNING_INFO"' EXIT; \
		for pid in $$RUNNING_PIDS; do \
			START_TIME="$$(awk '{print $$22}' "/proc/$$pid/stat" 2>/dev/null || true)"; \
			test -n "$$START_TIME" || continue; \
			if ! DUMP_OUT="$$(target/release/$(BIN_NAME) --dump-args "$$pid" 2>/dev/null)"; then \
				ACTUAL_START="$$(awk '{print $$22}' "/proc/$$pid/stat" 2>/dev/null || true)"; \
				ACTUAL_EXE="$$(readlink -f "/proc/$$pid/exe" 2>/dev/null || true)"; \
				if [ -n "$$ACTUAL_START" ] && [ "$$ACTUAL_START" = "$$START_TIME" ] && \
				   [ "$$ACTUAL_EXE" = "$$INSTALL_TARGET_REAL" ]; then \
					echo "ERROR: --dump-args failed for live drawer pid $$pid"; \
					exit 1; \
				fi; \
				continue; \
			fi; \
			printf "%s\t%s\n" "$$pid" "$$DUMP_OUT" >> "$$ARGS_FILE"; \
			echo "$$pid $$START_TIME" >> "$$RUNNING_INFO"; \
		done; \
		$(MAKE) install-bin install-data || exit 1; \
		VALIDATED_PIDS=""; \
		while IFS=' ' read -r pid start_time; do \
			ACTUAL_START="$$(awk '{print $$22}' "/proc/$$pid/stat" 2>/dev/null || true)"; \
			if [ -n "$$ACTUAL_START" ] && [ "$$ACTUAL_START" = "$$start_time" ]; then \
				VALIDATED_PIDS="$$VALIDATED_PIDS $$pid"; \
			else \
				echo "Skipping pid $$pid — no longer our drawer (starttime changed or process exited between capture and kill)"; \
			fi; \
		done < "$$RUNNING_INFO"; \
		if [ -n "$$VALIDATED_PIDS" ]; then \
			echo "Running instance(s):$$VALIDATED_PIDS — stopping"; \
			kill $$VALIDATED_PIDS 2>/dev/null || true; \
			sleep 1; \
			STILL_RUNNING=""; \
			for pid in $$VALIDATED_PIDS; do \
				START_TIME="$$(grep "^$$pid " "$$RUNNING_INFO" | awk '{print $$2}')"; \
				ACTUAL_START="$$(awk '{print $$22}' "/proc/$$pid/stat" 2>/dev/null || true)"; \
				if [ -n "$$ACTUAL_START" ] && [ "$$ACTUAL_START" = "$$START_TIME" ]; then \
					STILL_RUNNING="$$STILL_RUNNING $$pid"; \
				fi; \
			done; \
			if [ -n "$$STILL_RUNNING" ]; then \
				echo "Warning: still running after SIGTERM:$$STILL_RUNNING — escalating to SIGKILL"; \
				kill -9 $$STILL_RUNNING 2>/dev/null || true; \
				sleep 1; \
				FINAL_ALIVE=""; \
				for pid in $$STILL_RUNNING; do \
					START_TIME="$$(grep "^$$pid " "$$RUNNING_INFO" | awk '{print $$2}')"; \
					ACTUAL_START="$$(awk '{print $$22}' "/proc/$$pid/stat" 2>/dev/null || true)"; \
					if [ -n "$$ACTUAL_START" ] && [ "$$ACTUAL_START" = "$$START_TIME" ]; then \
						FINAL_ALIVE="$$FINAL_ALIVE $$pid"; \
					fi; \
				done; \
				test -z "$$FINAL_ALIVE" || { \
					echo "ERROR: failed to stop$$FINAL_ALIVE after SIGKILL; binary installed but drawer still holds old mmap"; \
					exit 1; \
				}; \
			fi; \
		fi; \
		if [ -n "$$VALIDATED_PIDS" ] && [ -s "$$ARGS_FILE" ]; then \
			if [ "$$(id -u)" -eq 0 ]; then \
				echo "Refusing to replay captured drawer args as root — the captured"; \
				echo "command came from the desktop user's process and running it via"; \
				echo "sh -c under elevated privileges would start the drawer in the"; \
				echo "wrong user context (and execute any desktop-env-derived values"; \
				echo "in that captured arg string). Install finished; restart the"; \
				echo "drawer manually from your desktop session."; \
			else \
				for pid in $$VALIDATED_PIDS; do \
					args="$$(awk -v p="$$pid" 'BEGIN{FS="\t"} $$1==p{sub(/^[^\t]*\t/, ""); print; exit}' "$$ARGS_FILE")"; \
					test -n "$$args" || continue; \
					echo "Restarting with captured args: $$args"; \
					setsid sh -c "$$args" </dev/null >/dev/null 2>&1 & \
				done; \
			fi; \
		fi; \
	else \
		echo "No running instance — installing; next drawer launch picks up the new binary"; \
		$(MAKE) install-bin install-data || exit 1; \
	fi
	@echo "Upgrade complete."

# ─────────────────────────────────────────────────────────────────────
# SonarQube scan — .env is PARSED (never sourced) to avoid shell injection.
# ─────────────────────────────────────────────────────────────────────

sonar:
	@echo "Running SonarQube scan..."
	@test -f ./.env || { echo "ERROR: .env not found in repo root"; exit 1; }
	@command -v "$(SONAR_SCANNER)" >/dev/null 2>&1 || [ -x "$(SONAR_SCANNER)" ] || { \
		echo "ERROR: sonar-scanner not found (looked at $(SONAR_SCANNER))"; exit 1; \
	}
	@test -r "$(SONAR_TRUSTSTORE)" || { \
		echo "ERROR: truststore not found or not readable at $(SONAR_TRUSTSTORE)"; \
		echo "  (sonar.aaru.network uses a self-signed cert — regenerate with:"; \
		echo "     openssl s_client -connect sonar.aaru.network:443 -showcerts </dev/null 2>/dev/null \\\\"; \
		echo "       | awk '/BEGIN CERT/,/END CERT/' > /tmp/sonar-cert.pem && \\\\"; \
		echo "     keytool -importcert -alias sonar-aaru -file /tmp/sonar-cert.pem \\\\"; \
		echo "       -keystore $(SONAR_TRUSTSTORE) -storepass $(SONAR_TRUSTSTORE_PASSWORD) -noprompt)"; \
		exit 1; \
	}
	@TOKEN="$$(awk '/^SONAR_TOKEN=/{sub(/^[^=]*=[ \t]*/, ""); sub(/[ \t]+$$/, ""); print; exit}' ./.env)"; \
	test -n "$$TOKEN" || { echo "ERROR: SONAR_TOKEN is empty in .env"; exit 1; }; \
	SONAR_TOKEN="$$TOKEN" \
	SONAR_SCANNER_OPTS="-Djavax.net.ssl.trustStore=$(SONAR_TRUSTSTORE) -Djavax.net.ssl.trustStorePassword=$(SONAR_TRUSTSTORE_PASSWORD)" \
	"$(SONAR_SCANNER)" -Dsonar.host.url="$(SONAR_HOST_URL)"

clean:
	$(CARGO) clean
