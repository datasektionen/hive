{
  pkgs ? (import <nixpkgs> { }),
  unstable ? (import <unstable> { }),
}:
pkgs.mkShellNoCC {
  buildInputs = with pkgs; [
    rustc
    cargo
    rustfmt

    rust-analyzer
    gcc # rust-analyzer needs cc linker

    cargo-make
  ];

  RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
}
