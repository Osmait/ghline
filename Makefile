# Thin wrapper over cargo. Nothing here does anything cargo cannot, it just
# saves remembering which flags each task wants.

BIN := ghline







.DEFAULT_GOAL := help
.PHONY: help install uninstall hooks build run diff test cov bench bench-cmp flame lint audit fmt check clean

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

run: ## Run ghline against real GitHub through gh
	cargo run --release --bin ghline

diff: ## Run diffline on this repository
	cargo run --release --bin diffline -- .

# CI runs these through `cargo nextest`, which reports the same pass or fail
# in a form that is easier to read when one of six hundred goes red. Plain
# `cargo test` is what is here because it needs nothing installed — install
# nextest and `cargo nextest run --all-targets` if you want CI's output.
test: ## The test suite
	cargo test

# Needs `cargo llvm-cov`, `cargo nextest` and the llvm-tools-preview
# component — CI installs all three, which is why they are not in
# rust-toolchain.toml. The total is the least interesting line of the report:
# read the columns for the file you just touched.
cov: ## What fraction of the crate the tests execute
	cargo llvm-cov nextest --all-targets --locked --summary-only

bench: ## What each layer costs, per module
	cargo bench

# One `cargo bench` is a number; two of them minutes apart is the only thing
# that answers "did that help?". The script does the stashing and the arithmetic
# and, more to the point, says whether the machine held still while it measured
# — a comparison run against a busy desk reads exactly like a real regression.
#
#   make bench-cmp            # every benchmark
#   make bench-cmp BENCH=draw # the ones whose name contains "draw"
bench-cmp: ## The same benchmarks either side of your uncommitted changes
	@scripts/bench-cmp.sh $(BENCH)

# The other half of `make bench`: that one says a frame costs 150µs, this says
# which line of it does. `perf` samples and `inferno` folds — neither is a
# cargo dependency, so both are checked for rather than assumed.
#
# The three environment variables are the whole trick. `[profile.release]`
# strips symbols and drops frame pointers because that is right for a tarball,
# and both of them are what a stack trace is made of: without them every
# sample folds into one box called `unknown`. Nothing else about the profile
# is changed, so what is measured is the binary that ships, inlining and all.
#
#   make flame              # every benchmark
#   make flame BENCH=draw   # the ones whose name contains "draw"
#
# `RUSTFLAGS` is part of cargo's cache key, so this and `make bench` evict
# each other and both rebuild from scratch when you alternate. That is the
# price of profiling the shipping profile rather than a debug one, and it is
# worth saying out loud rather than being discovered as a slow `make test`.
FLAME_ENV := RUSTFLAGS="-C force-frame-pointers=yes" \
             CARGO_PROFILE_BENCH_STRIP=none \
             CARGO_PROFILE_BENCH_DEBUG=line-tables-only
flame: ## Where the time inside a benchmark goes (needs perf and inferno)
	@command -v perf >/dev/null || { echo "  perf is not on the PATH"; exit 1; }
	@command -v inferno-flamegraph >/dev/null \
		|| { echo "  cargo install inferno"; exit 1; }
	@set -e; \
	bin=$$($(FLAME_ENV) cargo bench --no-run --message-format=json \
		| grep -o '"executable":"[^"]*/cost-[^"]*"' | tail -1 | cut -d'"' -f4); \
	test -n "$$bin" || { echo "  cargo built no bench binary"; exit 1; }; \
	echo "  sampling $$bin"; \
	perf record -q -F 1999 -g --call-graph fp -o target/flame.data -- \
		"$$bin" --bench --min-time 1 $(BENCH) >/dev/null; \
	perf script -i target/flame.data \
		| inferno-collapse-perf \
		| inferno-flamegraph --title "$(BIN) — $(if $(BENCH),$(BENCH),all benchmarks)" \
		> target/flame.svg
	@echo "  target/flame.svg — open it in a browser, the boxes are clickable"

lint: ## Formatting and lints, exactly as CI runs them
	cargo fmt --all --check
	cargo clippy --all-targets -- -D warnings
	# Again without the tests and benches, which is the shape that ships.
	cargo clippy -- -D warnings
	cargo doc --no-deps

# The last two are not cargo crates, and `uvx` runs them without installing
# anything permanently — the same versions CI pins actions to. Everything
# here is what the `deps` and `lint` jobs run.
#
# zizmor is pointed at `.github/` and not `.github/workflows/`, because that
# is what the action does and `dependabot.yml` is the difference between the
# two. Aimed at the narrower path this target passed while CI failed, which
# is the one way a check like this can be worse than not having it.
audit: ## Advisories, licences, sources, spelling and workflows
	cargo machete
	cargo deny --locked check
	uvx typos
	uvx zizmor --no-online-audits .github/

fmt: ## Apply the formatter
	cargo fmt --all

check: lint test ## Everything CI checks, before you push

clean: ## Remove the build directory
	cargo clean
