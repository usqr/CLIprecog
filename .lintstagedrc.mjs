export default {
  "*.{rs,toml}": () => [
    // Use RUSTFMT to point at nightly's rustfmt directly. `cargo +nightly`
    // requires rustup's cargo proxy on PATH, which is absent when cargo
    // comes from a non-rustup source (e.g. Homebrew).
    "sh -c 'RUSTFMT=$(rustup which --toolchain nightly rustfmt) cargo fmt -- --check --color always'",
    "cargo clippy --locked --color always -- -D warnings",
  ],
  "*.proto": () => [
    "cd proto && buf lint && buf format --exit-code > /dev/null",
  ],
  "*.py": ["ruff format --check", "ruff check"],
  "*.{ts,js,tsx,jsx,mjs}": "prettier --check",
  "!(*test*)*": "typos --config .typos.toml",
};
