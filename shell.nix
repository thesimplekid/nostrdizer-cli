{ pkgs ? import <nixpkgs> { } }:

with pkgs;

mkShell {
  nativeBuildInputs = [
    rustup
    cargo
    cargo-outdated
    rust-analyzer
    rustc
    rustfmt

    pkg-config
    openssl
  ];
}
