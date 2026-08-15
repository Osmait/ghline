# Thin wrapper over cargo. Nothing here does anything cargo cannot, it just
# saves remembering which flags each task wants.

BIN := github-tui







.DEFAULT_GOAL := help
.PHONY: help install uninstall hooks build run diff demo test test-nvim bench lint audit fmt check clean

# Two cargo processes would only queue on the target directory's lock, and the
# interleaved output would be unreadable. Nothing here is worth parallelising.
.NOTPARALLEL:

help: ## List what there is
	@grep -hE '^[a-z-]+:.*?## ' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-10s\033[0m %s\n", $$1, $$2}'

install: ## Build optimised and put it on the PATH (~/.cargo/bin)
	cargo install --path . --locked
	@echo
	@echo "  $(BIN) and diffline are on your PATH — run them from anywhere."
	@command -v $(BIN) >/dev/null || echo "  add ~/.cargo/bin to your PATH first"

uninstall: ## Take it off the PATH again
	cargo uninstall $(BIN)

hooks: ## Run `make check` on every push (git config core.hooksPath)
	git config core.hooksPath .githooks
	@echo "  pre-push now runs fmt, clippy and tests — skip once with --no-verify"

build: ## Optimised build, left in target/release
	cargo build --release

run: ## Run github-tui against real GitHub through gh
	cargo run --release --bin github-tui

diff: ## Run diffline on this repository
	cargo run --release --bin diffline -- .

demo: ## Run on the design's fixture, no network needed
	cargo run --release -- --demo

test: ## The test suite
	cargo test

bench: ## What a frame, the lexer, the matcher and wrapping cost
	cargo bench

test-nvim: ## The neovim plugin's tests (needs nvim and a running herdr)
	cd nvim/agent-send.nvim && nvim --headless -u NONE -c "set rtp+=." -c "luafile tests/run.lua"

lint: ## Formatting and lints, exactly as CI runs them
	cargo fmt --all --check
	cargo clippy --all-targets -- -D warnings
	cargo doc --no-deps

audit: ## Advisories, licences and sources (needs cargo-deny and cargo-machete)
	cargo machete
	cargo deny --locked check

fmt: ## Apply the formatter
	cargo fmt --all

check: lint test ## Everything CI checks, before you push

clean: ## Remove the build directory
	cargo clean
