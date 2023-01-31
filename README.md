# GISAID dupes

> Find duplicates in GISAID database dump.

## What's this

This is a small utility which allows you to search for entries with duplicate names in GISAIS NDJSON dump file.

## Build & run

```bash
# Clone Nextclade git repository
git clone https://github.com/nextstrain/nextclade
cd nextclade

# Install Rustup, the Rust version manager (https://www.rust-lang.org/tools/install)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add Rust tools to the $PATH
export PATH="$PATH:$HOME/.cargo/bin"

# Run in release mode (slow to build, fast to run)
cargo run --release --bin=gisaid-dupes -- gisaid.ndjson.zst -o gisaid.dupes.csv --verbose

# Alternatively, run in debug mode (fast to build, slow to run)
cargo run --bin=gisaid-dupes -- gisaid.ndjson.zst -o gisaid.dupes.csv --verbose

```
