default:
    @just --list

build:
    cargo build --all-targets

test:
    cargo test

# log_n 20 で暗号化する #[ignore] テスト。デバッグビルドでは数分かかる。
test-ignored:
    cargo test --release -- --ignored

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --all-targets -- -D warnings

# CI と同一の内容・同一の順序
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
