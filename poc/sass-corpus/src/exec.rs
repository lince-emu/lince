// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reference execution of a corpus program on the CPU, one thread, to compute its expected
//! output (decision D-005, doc 06 matching rule).
//!
//! Semantics are taken from each instruction's documented meaning, not from hardware runs:
//! IEEE 754 binary32 arithmetic with round-to-nearest-even and no flush-to-zero (`FADD`,
//! `FMUL`, `FFMA` with a single rounding), 32-bit wrapping integer addition, the `LOP3` truth
//! table indexed by `a·4 + b·2 + c` (NVIDIA PTX ISA, `lop3`), and 64-bit global addresses held
//! in a register pair. Behavior no public source defines here — other rounding modes, the bit
//! pattern of NaN results, reads of uninitialized memory — is refused rather than guessed
//! (`// UNKNOWN:` per AGENTS.md §3).

use std::collections::BTreeMap;

use crate::isa::{Cmp, Op, Operand, PT, Pred, RZ, Round};
use crate::parse::Program;

/// Instructions executed before a program is declared non-terminating.
const STEP_LIMIT: usize = 1_000_000;

/// Why a program could not be executed to completion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecError {
    /// The program ran past [`STEP_LIMIT`] instructions or off its end.
    NoExit,
    /// A load from a word the program never initialized or stored.
    Uninitialized(u64),
    /// An address that is not 4-byte aligned.
    Misaligned(u64),
    /// Behavior the corpus does not model because no public source defines it here.
    Unmodeled(&'static str),
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoExit => write!(f, "the program did not reach EXIT"),
            Self::Uninitialized(a) => write!(f, "load from uninitialized address {a:#x}"),
            Self::Misaligned(a) => write!(f, "misaligned address {a:#x}"),
            Self::Unmodeled(what) => write!(f, "not modeled: {what}"),
        }
    }
}

impl std::error::Error for ExecError {}

struct Machine {
    regs: [u32; 256],
    preds: [bool; 8],
    memory: BTreeMap<u64, u32>,
}

impl Machine {
    fn reg(&self, r: u8) -> u32 {
        if r == RZ {
            0
        } else {
            self.regs[usize::from(r)]
        }
    }

    fn set_reg(&mut self, r: u8, v: u32) {
        if r != RZ {
            self.regs[usize::from(r)] = v;
        }
    }

    fn operand(&self, o: Operand) -> u32 {
        match o {
            Operand::Reg(r) => self.reg(r),
            Operand::Imm(v) => v,
        }
    }

    fn pred(&self, p: Pred) -> bool {
        let v = p.reg == PT || self.preds[usize::from(p.reg)];
        v != p.not
    }

    fn address(&self, a: u8, offset: i32) -> Result<u64, ExecError> {
        let hi = if a == RZ {
            0
        } else {
            self.reg(a.wrapping_add(1))
        };
        let base = (u64::from(hi) << 32) | u64::from(self.reg(a));
        let addr = base.wrapping_add_signed(i64::from(offset));
        if addr % 4 == 0 {
            Ok(addr)
        } else {
            Err(ExecError::Misaligned(addr))
        }
    }
}

fn float(bits: u32) -> f32 {
    f32::from_bits(bits)
}

/// The bits of a float result; NaN results are refused because their bit pattern on the
/// hardware is not documented by any source the project may use.
fn float_result(v: f32, rnd: Round) -> Result<u32, ExecError> {
    if rnd != Round::Rn {
        // UNKNOWN: directed rounding is encoded but not modeled by the CPU reference.
        return Err(ExecError::Unmodeled("rounding modes other than RN"));
    }
    if v.is_nan() {
        // UNKNOWN: the NaN bit pattern produced by Ampere float units.
        return Err(ExecError::Unmodeled("NaN results"));
    }
    Ok(v.to_bits())
}

fn compare(cmp: Cmp, signed: bool, a: u32, b: u32) -> bool {
    let ord = if signed {
        a.cast_signed().cmp(&b.cast_signed())
    } else {
        a.cmp(&b)
    };
    match cmp {
        Cmp::Lt => ord.is_lt(),
        Cmp::Eq => ord.is_eq(),
        Cmp::Le => ord.is_le(),
        Cmp::Gt => ord.is_gt(),
        Cmp::Ne => ord.is_ne(),
        Cmp::Ge => ord.is_ge(),
    }
}

fn lop3(lut: u8, a: u32, b: u32, c: u32) -> u32 {
    (0..32).fold(0, |acc, bit| {
        let index = (((a >> bit) & 1) << 2) | (((b >> bit) & 1) << 1) | ((c >> bit) & 1);
        acc | (u32::from((lut >> index) & 1) << bit)
    })
}

/// Runs `program` from its first instruction until `EXIT` and returns the final contents of
/// every memory word it initialized or stored.
pub fn run(program: &Program) -> Result<BTreeMap<u64, u32>, ExecError> {
    let mut m = Machine {
        regs: [0; 256],
        preds: [false; 8],
        memory: program.memory.clone(),
    };
    let mut pc = 0usize;
    for _ in 0..STEP_LIMIT {
        let instr = program.instrs.get(pc).ok_or(ExecError::NoExit)?;
        pc += 1;
        if !m.pred(instr.guard) {
            continue;
        }
        match instr.op {
            Op::Mov { d, b } => m.set_reg(d, m.operand(b)),
            Op::Iadd3 { d, a, b, c } => {
                let v = m.reg(a).wrapping_add(m.operand(b)).wrapping_add(m.reg(c));
                m.set_reg(d, v);
            }
            Op::Lop3 { d, a, b, c, lut } => {
                m.set_reg(d, lop3(lut, m.reg(a), m.operand(b), m.reg(c)));
            }
            Op::Fadd { d, a, b, rnd } => {
                let v = float(m.reg(a)) + float(m.operand(b));
                m.set_reg(d, float_result(v, rnd)?);
            }
            Op::Fmul { d, a, b, rnd } => {
                let v = float(m.reg(a)) * float(m.operand(b));
                m.set_reg(d, float_result(v, rnd)?);
            }
            Op::Ffma { d, a, b, c, rnd } => {
                let v = float(m.reg(a)).mul_add(float(m.operand(b)), float(m.reg(c)));
                m.set_reg(d, float_result(v, rnd)?);
            }
            Op::Isetp {
                p,
                cmp,
                signed,
                a,
                b,
            } => {
                if p != PT {
                    m.preds[usize::from(p)] = compare(cmp, signed, m.reg(a), m.operand(b));
                }
            }
            Op::Bra { target } => pc = target,
            Op::Exit => return Ok(m.memory),
            Op::Ldg { d, a, offset } => {
                let addr = m.address(a, offset)?;
                let v = *m.memory.get(&addr).ok_or(ExecError::Uninitialized(addr))?;
                m.set_reg(d, v);
            }
            Op::Stg { a, v, offset } => {
                let addr = m.address(a, offset)?;
                m.memory.insert(addr, m.reg(v));
            }
        }
    }
    Err(ExecError::NoExit)
}

#[cfg(test)]
mod tests {
    use super::{ExecError, lop3, run};
    use crate::parse::parse;

    fn memory_after(src: &str) -> Vec<(u64, u32)> {
        run(&parse(src).unwrap()).unwrap().into_iter().collect()
    }

    #[test]
    fn lop3_truth_table_matches_the_classic_constants() {
        let (a, b, c) = (0xF0F0_F0F0, 0xCCCC_CCCC, 0xAAAA_AAAA);
        assert_eq!(lop3(0xF0, a, b, c), a);
        assert_eq!(lop3(0xCC, a, b, c), b);
        assert_eq!(lop3(0xAA, a, b, c), c);
        assert_eq!(lop3(0x96, a, b, c), a ^ b ^ c);
    }

    #[test]
    fn ffma_rounds_once() {
        // (1 + 2^-12)^2 - 1 = 2^-11 + 2^-24 exactly; rounding the product first (a tie, to
        // even) loses the 2^-24 term.
        let mem = memory_after(
            "MOV R0, 0x3f800800\nMOV R1, -1.0f\nFFMA R2, R0, R0, R1\nFMUL R3, R0, R0\n\
             FADD R3, R3, R1\nMOV R4, 0x100\nMOV R5, 0\nSTG.E.32 [R4], R2\n\
             STG.E.32 [R4+4], R3\nEXIT\n",
        );
        let fused = 2f32.powi(-11) + 2f32.powi(-24);
        assert_ne!(
            fused,
            2f32.powi(-11),
            "the fused value must be representable"
        );
        assert_eq!(
            mem,
            vec![(0x100, fused.to_bits()), (0x104, 2f32.powi(-11).to_bits())]
        );
    }

    #[test]
    fn guarded_branch_loops_until_the_predicate_fails() {
        let mem = memory_after(
            "MOV R0, 0\nMOV R1, 0\nloop:\nIADD3 R1, R1, R0, RZ\nIADD3 R0, R0, 1, RZ\n\
             ISETP.LT.U32 P0, R0, 5\n@P0 BRA loop\nMOV R4, 0x40\nSTG.E.32 [R4], R1\nEXIT\n",
        );
        assert_eq!(mem, vec![(0x40, 10)]);
    }

    #[test]
    fn unmodeled_behavior_is_refused() {
        let p = parse("MOV R0, 1.0f\nFADD.RZ R1, R0, R0\nEXIT\n").unwrap();
        assert!(matches!(run(&p), Err(ExecError::Unmodeled(_))));
        let p = parse("MOV R2, 0x100\nLDG.E.32 R0, [R2]\nEXIT\n").unwrap();
        assert_eq!(run(&p), Err(ExecError::Uninitialized(0x100)));
    }
}
