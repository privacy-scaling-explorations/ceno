> **Archiving Notice Apr 2025** ⚠️:
> 
> PSE collaborated with [Scroll](https://scroll.io) from  2024Q1 -  2025Q1 building the first iteration of Ceno. Special thanks to [Ming](https://github.com/hero78119) for leading that effort, [Kimi](https://github.com/KimiWu123) and [Soham](https://github.com/zemse) for their significant dev contributions, [Even](https://github.com/10to4) for help on the research side, and [Edu](https://github.com/ed255) and [Ali](https://github.com/0xalizk) for their general support.
>
> For the latest on Ceno please follow the [upstream repo](https://github.com/scroll-tech/ceno). 

# Ceno: Non-uniform, Segment and Parallel Risc-V Zero-knowledge Virtual Machine

Please see [the slightly outdated paper](https://eprint.iacr.org/2024/387) for an introduction to Ceno.

🚧 This project is currently under construction and not suitable for use in production. 🚧

If you are unfamiliar with the RISC-V instruction set, please have a look at the [RISC-V instruction set reference](https://github.com/jameslzhu/riscv-card/blob/master/riscv-card.pdf).

## Local build requirements

Ceno is built in Rust, so [installing the Rust toolchain](https://www.rust-lang.org/tools/install) is a pre-requisite, if you want to develop on your local machine.  We also use [cargo-make](https://sagiegurari.github.io/cargo-make/) to build Ceno. You can install cargo-make with the following command:

```sh
cargo install cargo-make
```

You will also need to install the Risc-V target for Rust. You can do this with the following command:

```sh
rustup target add riscv32im-unknown-none-elf
```

## Building Ceno and running tests

To run the tests, you can use the following command:

```sh
cargo make tests
```

Clippy and check work as usual:

```sh
cargo check
cargo clippy
```

Alas, `cargo build` doesn't work. That's a known problem and we're working on it.  Please use `cargo make build` instead for now.
