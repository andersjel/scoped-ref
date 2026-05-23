# The [`scoped-ref`](https://docs.rs/scoped-ref/) Crate

[![Crates.io Version](https://img.shields.io/crates/v/scoped-ref.svg)](https://crates.io/crates/scoped-ref)
[![Docs.rs](https://docs.rs/scoped-ref/badge.svg)](https://docs.rs/scoped-ref)
[![CI Status](https://github.com/andersjel/scoped-ref/actions/workflows/ci.yml/badge.svg)](https://github.com/andersjel/scoped-ref/actions/workflows/ci.yml)
[![License](https://img.shields.io/crates/l/scoped-ref.svg)](https://github.com/andersjel/scoped-ref/blob/main/LICENSE-MIT)

This crate provides safe references with runtime-checked lifetimes. While a reference is in use, attempts to drop the referenced value will block.

Refer to docs.rs for the [full documentation](https://docs.rs/scoped-ref/).
