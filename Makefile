BUILD_TYPE ?= release
ifeq ($(BUILD_TYPE), release)
	CARGO_BUILD_TYPE = --release
endif
LOG_LEVEL ?= trace
CONFIG_FILE ?= vpn.toml
HOSTS_CONFIG_FILE ?= hosts.toml
ENDPOINT_HOSTNAME ?= vpn.example.invalid
LISTEN_ADDRESS ?= 0.0.0.0
LISTEN_PORT ?= 443

.PHONY: init
## Initialize the development environment (git hooks, etc.)
init:
	git config core.hooksPath ./scripts/hooks

.PHONY: endpoint/build-wizard
## Build the setup wizard
endpoint/build-wizard:
	cargo build --locked $(CARGO_BUILD_TYPE) --bin setup_wizard

.PHONY: endpoint/setup
## Run the setup wizard to create all the required configuration files
endpoint/setup: endpoint/build-wizard
	cargo run --locked $(CARGO_BUILD_TYPE) --bin setup_wizard -- \
		--hostname "$(ENDPOINT_HOSTNAME)" \
		--address "$(LISTEN_ADDRESS):$(LISTEN_PORT)" \
		--lib-settings "$(CONFIG_FILE)" \
		--hosts-settings "$(HOSTS_CONFIG_FILE)"

.PHONY: endpoint/build
## Build the endpoint
endpoint/build:
	cargo build --locked $(CARGO_BUILD_TYPE) --bin trusttunnel_endpoint

.PHONY: endpoint/run
## Run the endpoint with the existing configuration files
endpoint/run: endpoint/build
	cargo run --locked $(CARGO_BUILD_TYPE) --bin trusttunnel_endpoint -- \
		-l "$(LOG_LEVEL)" "$(CONFIG_FILE)" "$(HOSTS_CONFIG_FILE)"

.PHONY: endpoint/gen_client_config
## Generate the config for specified client to be used with vpn client and exit
endpoint/gen_client_config:
	$(if $(CLIENT_NAME),,$(error CLIENT_NAME is not set. Specify the client name to generate the config for))
	$(if $(ENDPOINT_ADDRESS),,$(error ENDPOINT_ADDRESS is not set. Set it to `ip:port` that client is going to use to connect to the endpoint))
	cargo run --locked $(CARGO_BUILD_TYPE) --bin trusttunnel_endpoint -- \
		-c "$(CLIENT_NAME)" --address "$(ENDPOINT_ADDRESS)" "$(CONFIG_FILE)" "$(HOSTS_CONFIG_FILE)"

.PHONY: endpoint/clean
## Clean cargo artifacts
endpoint/clean:
	cargo clean

.PHONY: lint
lint: lint-md lint-rust

## Lint markdown files.
## `markdownlint-cli` should be installed:
##    macOS: `brew install markdownlint-cli`
##    Linux: `npm install -g markdownlint-cli`
.PHONY: lint-md
lint-md:
	markdownlint .

## Check Rust code formatting with rustfmt.
## `rustfmt` should be installed:
##    rustup component add rustfmt
.PHONY: lint-rust
lint-rust:
	cargo fmt --all -- --check
	cargo clippy --locked -- -D warnings

## Fix linter issues that are auto-fixable.
.PHONY: lint-fix
lint-fix: lint-fix-rust lint-fix-md

## Auto-fix Rust code formatting issues with rustfmt.
.PHONY: lint-fix-rust
lint-fix-rust:
	cargo clippy --fix --allow-dirty
	cargo fmt --all

## Auto-fix markdown files.
.PHONY: lint-fix-md
lint-fix-md:
	markdownlint --fix .

.PHONY: test
test: test-rust test-source-policy

test-rust:
	cargo test --locked --workspace

.PHONY: test-source-policy
test-source-policy:
	PYTHONPATH=scripts python3 -m unittest discover -s scripts -p 'test_*.py'

.PHONY: audit-source
audit-source:
	python3 scripts/audit_source_tree.py
	python3 scripts/audit_runtime_policy.py
	python3 scripts/audit_build_inputs.py
