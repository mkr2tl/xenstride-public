// Modified by Xenstride on 2026-11-25. See runtime/XENSTRIDE_CHANGES.md.
//! Xenstride P16: how much per-lane private memory (`xcl_scratch`) a program provably uses.
//!
//! `scratch_*` instructions address `base register + offset`; the translator declared 1024 dwords
//! per lane for every program that has one (4 KiB per lane). On an AMD driver the first dispatch of
//! such a program allocates scratch for the whole GPU's occupancy: a bounded P15 control measured
//! +257 MiB of local video memory and +264 MiB of process commit for 1024 private dwords, 0 for 64.
//!
//! The array can be smaller with the same results when every access is proven below its size:
//! with all dword indices `b` (and `b + 1` for the sub-dword helpers) under `n`, `& (n - 1)` and
//! `& 1023` pick the same element, and elements at or above `n` are never read or written by either.
//! The proof here is deliberately narrow; anything it does not prove keeps 1024 dwords:
//! - every scratch base register has exactly one writer in the whole program, an instruction of a
//!   few recognized 32-bit operations, in the entry region (before any branch, branch target or
//!   EXEC write: every active lane runs it) and before every use; its sources are constants or
//!   registers proven the same way (or the result is bounded by a constant mask);
//! - no instruction writes registers indirectly (v_movreld / v_movrelsd / v_swaprel / s_movreld);
//! - offsets are non-negative.

use crate::gpu::rdna2::{self, Decoded};
use std::collections::{HashMap, HashSet};

/// Dwords the full-size array has (and the result when nothing is proven).
pub const FULL: u32 = 1024;

/// Register code: SGPR / special codes 0..255, VGPR n = 256 + n.
type Reg = u64;

fn name(d: &Decoded) -> String {
    d.inst.name.to_ascii_lowercase()
}

/// The registers an instruction may write (`None`: any VGPR, indirect write).
fn writes(d: &Decoded) -> Option<Vec<Reg>> {
    let n = name(d);
    if n.starts_with("v_movreld") || n.starts_with("v_movrelsd") || n == "v_swaprel_b32" || n.starts_with("s_movreld") || n.starts_with("s_movrelsd") {
        return None;
    }
    let mut out = Vec::new();
    for o in d.ienc.operands.iter().filter(|o| o.output) {
        let dwords = ((o.size as u64).max(32) + 31) / 32;
        match (o.field, o.ty) {
            (_, "OPR_VCC") => out.extend([106, 107]),
            (_, "OPR_EXEC") => out.extend([126, 127]),
            (None, "OPR_SDST_M0") => out.push(124),
            (None, _) => {}
            (Some(f), ty) => {
                let Some(v) = rdna2::field_value(d.enc, &d.words, f) else { continue };
                let base = if ty.contains("VGPR") { 256 + v } else { v };
                out.extend(base..base + dwords.min(16));
            }
        }
    }
    Some(out)
}

fn is_exec_write(regs: &[Reg]) -> bool {
    regs.iter().any(|r| *r == 126 || *r == 127)
}

/// A 32-bit source operand's upper bound (`regs`: bounds of proven registers).
fn source(code: u64, literal: Option<u32>, proven: &HashMap<Reg, u64>, vgpr_field: bool) -> Option<u64> {
    if vgpr_field {
        return proven.get(&(256 + code)).copied();
    }
    match code {
        128..=192 => Some(code - 128),
        255 => literal.map(u64::from),
        256..=511 => proven.get(&code).copied(),
        0..=105 => proven.get(&code).copied(),
        _ => None,
    }
}

/// The bound of the one recognized operation that writes `dst`, from its sources.
fn eval(d: &Decoded, proven: &HashMap<Reg, u64>) -> Option<u64> {
    let f = |n: &str| rdna2::field_value(d.enc, &d.words, n);
    let n = name(d);
    let vop2 = d.enc.name == "ENC_VOP2";
    let vop3 = d.enc.name == "ENC_VOP3";
    // (operand code, given as a VGPR index rather than a source code)
    let s0 = || f("SRC0").or(f("SSRC0")).and_then(|c| source(c, d.literal, proven, false));
    let s1 = || {
        if vop2 {
            f("VSRC1").and_then(|c| source(c, d.literal, proven, true))
        } else {
            f("SRC1").or(f("SSRC1")).and_then(|c| source(c, d.literal, proven, false))
        }
    };
    if vop3 && (f("ABS").unwrap_or(0) != 0 || f("NEG").unwrap_or(0) != 0 || f("OMOD").unwrap_or(0) != 0 || f("CLAMP").unwrap_or(0) != 0) {
        return None;
    }
    let lim = |v: u64| (v <= u32::MAX as u64).then_some(v);
    match n.as_str() {
        "v_mov_b32" | "s_mov_b32" => s0(),
        "v_and_b32" | "s_and_b32" => match (s0(), s1()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) | (None, Some(a)) => Some(a),
            _ => None,
        },
        "v_lshlrev_b32" => lim(s1()? << s0()?.min(31)),
        "s_lshl_b32" => lim(s0()? << s1()?.min(31)),
        "v_lshrrev_b32" => s1(),
        "s_lshr_b32" => s0(),
        "v_add_nc_u32" | "v_add_u32" | "s_add_u32" => lim(s0()? + s1()?),
        "v_mul_u32_u24" => lim((s0()? & 0xFF_FFFF) * (s1()? & 0xFF_FFFF)),
        _ => None,
    }
}

/// The dwords of `xcl_scratch` this program needs: a power of two (4 .. 1024); `FULL` when the
/// addresses are not all proven.
pub fn dwords(insts: &[Decoded]) -> u32 {
    // the entry region: before any branch, branch target or EXEC write
    let targets: HashSet<u64> = insts.iter().filter_map(rdna2::branch_target).collect();
    let mut entry_end = insts.len();
    let mut writers: HashMap<Reg, Vec<usize>> = HashMap::new();
    for (i, d) in insts.iter().enumerate() {
        let Some(w) = writes(d) else { return FULL };
        let n = name(d);
        let branch = n.starts_with("s_branch") || n.starts_with("s_cbranch") || n.starts_with("s_setpc") || n.starts_with("s_swappc") || n.starts_with("s_call");
        if entry_end == insts.len() && (targets.contains(&d.pc) || branch || is_exec_write(&w)) {
            entry_end = i;
        }
        for r in w {
            writers.entry(r).or_default().push(i);
        }
    }
    // registers proven by their single writer in the entry region, in program order
    let mut proven: HashMap<Reg, u64> = HashMap::new();
    for (i, d) in insts.iter().enumerate().take(entry_end) {
        let Some(w) = writes(d) else { return FULL };
        if w.len() != 1 || writers.get(&w[0]).map_or(true, |v| v.len() != 1) {
            continue;
        }
        if let Some(b) = eval(d, &proven) {
            proven.insert(w[0], b);
        }
        let _ = i;
    }
    let mut need = 0u64;
    for (i, d) in insts.iter().enumerate() {
        if d.enc.name != "ENC_FLAT_SCRATCH" {
            continue;
        }
        let f = |n: &str| rdna2::field_value(d.enc, &d.words, n).unwrap_or(0);
        let saddr = f("SADDR");
        let off = ((((f("OFFSET") as u32) << 20) as i32) >> 20) as i64;
        if off < 0 {
            return FULL;
        }
        // ST mode (SADDR 0x7F): the offset alone; VADDR (0x7D) or an SGPR base otherwise
        let bound = if saddr == 0x7F {
            0
        } else {
            let base: Reg = if saddr == 0x7D { 256 + f("ADDR") } else { saddr };
            let Some(&bound) = proven.get(&base) else { return FULL };
            // the writer runs before this use
            if writers[&base][0] >= i {
                return FULL;
            }
            bound
        };
        let n = name(d);
        let bytes = if n.contains("dwordx4") { 16 } else if n.contains("dwordx3") { 12 } else if n.contains("dwordx2") { 8 } else { 4 };
        // highest byte + 4: the sub-dword helpers also touch the next dword
        need = need.max(bound + off as u64 + bytes + 4);
    }
    if need == 0 {
        return FULL;
    }
    let dw = need.div_ceil(4).next_power_of_two().max(4);
    dw.min(FULL as u64) as u32
}

thread_local! {
    /// (tests) translate with the full-size array on this thread
    static FORCE_FULL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// `XCL_SCRATCH_FULL=1`, or a test asked for the full-size array on this thread.
pub fn forced_full() -> bool {
    static ENV: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    FORCE_FULL.with(|f| f.get()) || *ENV.get_or_init(|| std::env::var("XCL_SCRATCH_FULL").is_ok_and(|v| v.trim() == "1"))
}

/// (tests) Translate on this thread with the full-size array (`full`) or the proven size.
pub fn set_forced_full_for_tests(full: bool) {
    FORCE_FULL.with(|f| f.set(full));
}
