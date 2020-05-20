# cargo watt

> Watt is a runtime for executing Rust procedural macros compiled as WebAssembly.

I assume you are familiar with [watt](https://github.com/dtolnay/watt/blob/master/README.md), dtolnay's crate for executing procedural macros in a [web assembly](https://webassembly.org/) interpreter.

There, tooling improvements are listed as _remaining work_, and this cargo subcommand aims to achieve that.
Its purposes are

1. build proc-macro crates without manual intervention for the watt runtime
2. verify that a wasm file is compiled from a particular source

# Building proc-macro crates

Building works by first copying a crate (either from a local directory, a git repository or crates.io) into `/tmp`.
The crate type is then changed to `cdylib` and patches for `proc-macro2` and `syn` are being applied.

The [syn patch](https://github.com/jakobhellermann/syn-watt) is needed because in wasm there is no `proc-macro` crate, which syn exposes e.g. in the [`proc_macro_input!`](https://docs.rs/syn/1.0.22/syn/macro.parse_macro_input.html) macro.
The patched version of basically syn has all instances of `proc_macro` replaced with `proc_macro2` and the conditional compilation for `wasm32-unknown-unknown` is removed.

Then all procedural macros in it are being replaced with `rust #[no_mangle] extern "C" fn`s and a shim crate is generated which calls into the generated web assembly file and exetutes the token tree transformation.

As a user, all you need to do is

```sh
$ cargo watt --crate serde-derive
  INFO  cargo_watt > download crate 'serde-derive' into temporary directory...
  INFO  cargo_watt > begin compiling crate...
     Updating git repository `https://github.com/dtolnay/watt`
     Updating git repository `https://github.com/jakobhellermann/syn-watt`
     Updating crates.io index
    Compiling syn v1.0.22 (https://github.com/jakobhellermann/syn-watt#0f0ace5e)
    Compiling serde_derive v1.0.110 (/tmp/cargo-watt-crate)
     Finished release [optimized] target(s) in 19.65s
  INFO  cargo_watt > finished in 19.65s
  INFO  cargo_watt > compiled wasm file is 2.65mb large
  INFO  cargo_watt > generated crate in "serde_derive-watt"
```

Alternatively you can fetch a git repository (`cargo watt --git https://github.com/idanarye/rust-typed-builder`) or use a local path (`cargo watt ./path/to/crate`).

By default, `cargo watt` will include all files of original crate (i.e. tests, documentation etc.) in the newly generated one.
If you'd like to only have `Cargo.toml`, `src/lib.rs` and `src/the-macro.wasm` there is the `--only-copy-essential` option.

## Caveats

Some proc-macro crates need to export other things then the actual macros, so they are split into a regular rust crate exporting some Traits/Functions, which then reexports the macros from another crate.

This is why `cargo watt --crate thiserror` will tell you that thiserror is not a proc macro crate.

Instead you would need to do `cargo watt --crate thiserror-impl`, clone `thiserror` and change it's `impl`-dependency to our generated watt crate.

Maybe this will be automated by `cargo watt` in the future but until then this is a limitation.

# Verifying compilation

** Todo **

<br>

#### LICENSE

MIT © [Jakob Hellermann](mailto:jakob.hellermann@protonmail.com)
