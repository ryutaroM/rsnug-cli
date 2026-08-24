default:
    @just --list

build:
    cargo build --all-targets

test:
    cargo test

# The #[ignore] test encrypts at log_n 20, which takes minutes in a debug build.
test-ignored:
    cargo test --release -- --ignored

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --all-targets -- -D warnings

# The same checks in the same order as the CI workflow.
ci: build test fmt-check lint

build-release:
    cargo build --release

run *args:
    cargo run -- {{ args }}

version-check tag:
    #!/usr/bin/env bash
    set -euo pipefail
    tag="{{ tag }}"
    tag_version="${tag#v}"
    cargo_version="$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[0].version')"
    if [ "$tag_version" != "$cargo_version" ]; then
        echo "tag $tag does not match Cargo.toml version $cargo_version" >&2
        exit 1
    fi

docker +args:
    docker compose run --rm dev {{ args }}
