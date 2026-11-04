## static binary

Build a statically linked Linux binary with the `musl` target. Install the Rust target and a musl linker first (on Debian/Ubuntu, `musl-tools`):

```sh
rustup target add x86_64-unknown-linux-musl
sudo apt install musl-tools
```

Then build by passing the target on the command line:

```sh
cargo build --release --target x86_64-unknown-linux-musl
```

The binary is written to `target/x86_64-unknown-linux-musl/release/countrycode`.

To make this the default target for this project, add the following to `.cargo/config.toml`:

```toml
[build]
target = "x86_64-unknown-linux-musl"
```

After that, `cargo build --release` uses the musl target automatically. Create the `.cargo` directory if it does not exist. Choose the musl target matching the machine architecture when building for a different platform.
