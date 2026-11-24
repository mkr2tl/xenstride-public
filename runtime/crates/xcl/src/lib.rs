// Xenstride notice (GPL-2.0-only, section 2a): this file was changed relative to ivall/xcl 9aec53d on 2026-11-23; see XENSTRIDE_CHANGES.md at the runtime root.
//! XCL — Xbox Series X|S → Windows compatibility layer.
//!
//! See `docs/specs/2026-11-20-xcl-design.md` for the design.

// Function → address casts are the whole point of an import resolver.
#![allow(function_casts_as_integer)]

#[macro_use]
pub mod log;
pub mod manifest;
pub mod config;
pub mod disasm;
pub mod eh;
pub mod diagnostics;
pub mod gpu;
pub mod hle;
pub mod hooks;
pub mod host;
pub mod insn;
pub mod inspect;
pub mod launcher;
pub mod loader;
pub mod modules;
pub mod pe;
pub mod preflight;
pub mod report;
pub mod resolver;
pub mod rpf;
mod rpf_probe;
pub mod shaders;
pub mod state;
pub mod stubs;
pub mod tls;
pub mod util;
pub mod vfs;
pub mod vfs_hooks;
