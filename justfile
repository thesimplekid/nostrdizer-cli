check:
    cargo fmt --check --all
    cargo clippy --all
test:
    cargo test --workspace