# Stack

This document gives the engine, the crates, and the versions of this repository.

## Versions

| Area | Crate | Version | Reason |
| --- | --- | --- | --- |
| Engine | `bevy` | 0.19.1 | The newest Bevy release that all crates below support. |
| Physics | `avian3d` | 0.7.0 | Avian keeps its physics data in Bevy components. Replication of these components is simple. |
| Replication | `bevy_replicon` | 0.44.2 | Replicon supports a listen server. It does no rollback. |
| Transport | `bevy_replicon_renet` | 0.20.0 | This crate connects Replicon to renet. The `renet_steam` feature adds the Steam transport. |
| Steam | `steamworks` | 0.12.2 | The Steam transport of renet uses this version. |

All versions are exact. The minimum Rust version is 1.95.

## The network model

The host simulates all physics. A client sends its input to the host. The host replicates the state to each client.

Replicon does not predict movement. The game predicts the movement of the local pigeon. Then the game corrects the prediction softly to the state of the host.

## Steam

The game uses the Steam relay to connect the players. The relay removes the need to open a port on the router.

The game does not use `bevy_steamworks`. Version 0.17 of `bevy_steamworks` requires `steamworks` 0.13. The Steam transport of renet requires `steamworks` 0.12. Two versions of the Steam SDK in one program are not tested. The game calls `steamworks` 0.12 directly.

## Upgrade rule

Upgrade the versions only between milestones. Upgrade all crates of this table in one pull request.

Before an upgrade, make sure that each crate supports the new Bevy version. If one crate does not support it, do not upgrade.

If `bevy_replicon_renet` moves to `steamworks` 0.13, examine `bevy_steamworks` again.

## The toolchain

`rust-toolchain.toml` selects the nightly toolchain. Rustup installs rustfmt, Clippy, and the Cranelift code generator with it.

## Fast development builds

A development build uses the Cranelift code generator. Cranelift generates code faster than LLVM. `.cargo/config.toml` sets it.

The `poly1305` crate uses AVX2 instructions. Cranelift cannot compile them, so `poly1305` uses LLVM.

Do not use sccache. On Windows, sccache cannot start rustc for the large Bevy crates. The command line is too long.

The `dev` feature links Bevy as a shared library. On Windows, Cranelift cannot link this library. To use the `dev` feature, set `codegen-backend = "llvm"` for the dev profile.

## Release builds

A release build uses fat LTO and one code generation unit. The build is slow, and the game is fast.

Do not use the `dev` feature for a release build. A release build includes Bevy in the program file.
