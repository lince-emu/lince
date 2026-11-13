// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Instructions of the corpus subset and their 128-bit Ampere (SM 8.x) encoding.
//!
//! Every bit position and opcode value comes from `docs/research/ampere-sass-encoding.md`,
//! which cites its source per field (decision D-011). The encoder is written from that note,
//! not from any other project's code (LEGAL-20/21).

/// Predicate register that always reads true.
pub const PT: u8 = 7;
/// GPR that reads as zero and discards writes.
pub const RZ: u8 = 255;
/// Uniform register that reads as zero (SM 8.x encodes uniform registers in 6 bits).
const URZ: u8 = 63;

/// A guard or condition predicate: register `reg`, optionally negated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pred {
    /// Predicate register, 0–7 (7 = `PT`).
    pub reg: u8,
    /// Use the complement.
    pub not: bool,
}

impl Pred {
    /// `PT`: always true.
    pub const TRUE: Self = Self {
        reg: PT,
        not: false,
    };
}

/// A GPR or 32-bit immediate operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operand {
    /// General-purpose register 0–255 (255 = `RZ`).
    Reg(u8),
    /// 32-bit immediate (raw bits; floats are stored as their IEEE 754 bits).
    Imm(u32),
}

/// Integer comparison of `ISETP`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    /// `a < b`
    Lt,
    /// `a == b`
    Eq,
    /// `a <= b`
    Le,
    /// `a > b`
    Gt,
    /// `a != b`
    Ne,
    /// `a >= b`
    Ge,
}

/// Floating-point rounding mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Round {
    /// To nearest, ties to even.
    Rn,
    /// Toward negative infinity.
    Rm,
    /// Toward positive infinity.
    Rp,
    /// Toward zero.
    Rz,
}

/// One instruction of the subset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// `MOV d, b`
    Mov {
        /// Destination GPR.
        d: u8,
        /// Second source: a GPR or a 32-bit immediate.
        b: Operand,
    },
    /// `IADD3 d, a, b, c`: 32-bit wrapping sum.
    Iadd3 {
        /// Destination GPR.
        d: u8,
        /// First source GPR.
        a: u8,
        /// Second source: a GPR or a 32-bit immediate.
        b: Operand,
        /// Third source GPR.
        c: u8,
    },
    /// `LOP3.LUT d, a, b, c, lut`: bitwise function given by an 8-bit truth table.
    Lop3 {
        /// Destination GPR.
        d: u8,
        /// First source GPR.
        a: u8,
        /// Second source: a GPR or a 32-bit immediate.
        b: Operand,
        /// Third source GPR.
        c: u8,
        /// Truth table, indexed by `a·4 + b·2 + c` per bit.
        lut: u8,
    },
    /// `FADD d, a, b`
    Fadd {
        /// Destination GPR.
        d: u8,
        /// First source GPR.
        a: u8,
        /// Second source: a GPR or a 32-bit immediate.
        b: Operand,
        /// Rounding mode.
        rnd: Round,
    },
    /// `FMUL d, a, b`
    Fmul {
        /// Destination GPR.
        d: u8,
        /// First source GPR.
        a: u8,
        /// Second source: a GPR or a 32-bit immediate.
        b: Operand,
        /// Rounding mode.
        rnd: Round,
    },
    /// `FFMA d, a, b, c`: `a * b + c` with one rounding.
    Ffma {
        /// Destination GPR.
        d: u8,
        /// First source GPR.
        a: u8,
        /// Second source: a GPR or a 32-bit immediate.
        b: Operand,
        /// Third source GPR.
        c: u8,
        /// Rounding mode.
        rnd: Round,
    },
    /// `ISETP.<cmp>.<U32|I32>.AND p, PT, a, b, PT`
    Isetp {
        /// Destination predicate.
        p: u8,
        /// Comparison.
        cmp: Cmp,
        /// Compare as `I32` (true) or as `U32` (false).
        signed: bool,
        /// First source GPR.
        a: u8,
        /// Second source: a GPR or a 32-bit immediate.
        b: Operand,
    },
    /// `BRA target` (target is an instruction index; the guard predicate makes it conditional).
    Bra {
        /// Index of the instruction to jump to.
        target: usize,
    },
    /// `EXIT`
    Exit,
    /// `LDG.E.32 d, [a.64 + offset]`
    Ldg {
        /// Destination GPR.
        d: u8,
        /// Low GPR of the 64-bit address pair.
        a: u8,
        /// Signed byte offset, 24 bits.
        offset: i32,
    },
    /// `STG.E.32 [a.64 + offset], v`
    Stg {
        /// Low GPR of the 64-bit address pair.
        a: u8,
        /// GPR holding the value to store.
        v: u8,
        /// Signed byte offset, 24 bits.
        offset: i32,
    },
}

/// Scheduling control of one instruction (bits 105–125).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Control {
    /// Stall cycles before the next instruction, 0–15.
    pub delay: u8,
    /// Scoreboard this instruction sets when its result is written, 0–5 or none.
    pub write_sb: Option<u8>,
    /// Scoreboard this instruction sets when its sources have been read, 0–5 or none.
    pub read_sb: Option<u8>,
    /// Scoreboards to wait on before issuing, bit mask of 0–5.
    pub wait: u8,
    /// Allow the scheduler to switch warps after this instruction.
    pub yield_: bool,
}

impl Control {
    /// The longest stall and no scoreboards: always safe, never fast.
    pub const DEFAULT: Self = Self {
        delay: 15,
        write_sb: None,
        read_sb: None,
        wait: 0,
        yield_: false,
    };
}

/// An instruction with its guard predicate and scheduling control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Instr {
    /// The operation.
    pub op: Op,
    /// Guard: the instruction runs only where this predicate is true.
    pub guard: Pred,
    /// Scheduling control.
    pub ctl: Control,
}

/// A 128-bit instruction being built field by field.
struct Word([u32; 4]);

impl Word {
    fn set(&mut self, lo: usize, width: usize, value: u64) {
        debug_assert!(width <= 48 && lo + width <= 128);
        debug_assert!(
            value < 1u64 << width,
            "value {value:#x} does not fit {width} bits"
        );
        for i in 0..width {
            let bit = lo + i;
            let on = (value >> i) & 1 == 1;
            let mask = 1u32 << (bit % 32);
            if on {
                self.0[bit / 32] |= mask;
            } else {
                self.0[bit / 32] &= !mask;
            }
        }
    }

    fn flag(&mut self, bit: usize, on: bool) {
        self.set(bit, 1, u64::from(on));
    }

    fn pred(&mut self, lo: usize, not_bit: usize, p: Pred) {
        self.set(lo, 3, u64::from(p.reg));
        self.flag(not_bit, p.not);
    }

    /// ALU opcode (bits 0–8), destination, `src0` and the operand form (bits 9–11).
    fn alu(&mut self, opcode: u16, d: Option<u8>, a: Option<u8>, (b, c): (Operand, Option<u8>)) {
        self.set(0, 9, u64::from(opcode));
        if let Some(d) = d {
            self.set(16, 8, u64::from(d));
        }
        if let Some(a) = a {
            self.set(24, 8, u64::from(a));
        }
        let form = match (b, c) {
            (Operand::Reg(b), c) => {
                self.set(32, 8, u64::from(b));
                if let Some(c) = c {
                    self.set(64, 8, u64::from(c));
                }
                1
            }
            (Operand::Imm(imm), c) => {
                self.set(32, 32, u64::from(imm));
                if let Some(c) = c {
                    self.set(64, 8, u64::from(c));
                }
                4
            }
        };
        self.set(9, 3, form);
    }

    /// Unused carry/accumulator predicate inputs read `!PT` (false).
    fn false_pred(&mut self, lo: usize, not_bit: usize) {
        self.pred(lo, not_bit, Pred { reg: PT, not: true });
    }

    fn global_access(&mut self, opcode: u16, a: u8, offset: i32, uniform_addr_at: usize) {
        self.set(0, 12, u64::from(opcode));
        self.set(24, 8, u64::from(a));
        self.set(uniform_addr_at, 6, u64::from(URZ));
        self.set(40, 24, u64::from(offset.cast_unsigned() & 0x00FF_FFFF));
        self.flag(72, true); // the zero uniform address counts as 64-bit
        self.set(73, 3, 4); // B32
        self.set(77, 4, 0); // weak order
        self.set(84, 3, 1); // normal eviction priority
        self.flag(90, true); // the address GPR pair is 64-bit
        self.flag(91, true);
    }
}

fn round_bits(r: Round) -> u64 {
    match r {
        Round::Rn => 0,
        Round::Rm => 1,
        Round::Rp => 2,
        Round::Rz => 3,
    }
}

fn cmp_bits(c: Cmp) -> u64 {
    match c {
        Cmp::Lt => 1,
        Cmp::Eq => 2,
        Cmp::Le => 3,
        Cmp::Gt => 4,
        Cmp::Ne => 5,
        Cmp::Ge => 6,
    }
}

/// Floating-point modifier bits shared by `FADD`, `FMUL` and `FFMA`: no saturation, the
/// rounding mode, no flush-to-zero.
fn float_mods(w: &mut Word, rnd: Round) {
    w.flag(77, false);
    w.set(78, 2, round_bits(rnd));
    w.flag(80, false);
}

/// Encodes `instr`, which is instruction number `index` of its program.
///
/// # Panics
/// If a field does not fit, for example a branch target too far away; the parser rejects
/// such programs first.
pub fn encode(instr: &Instr, index: usize) -> [u32; 4] {
    let mut w = Word([0; 4]);
    match instr.op {
        Op::Bra { .. } | Op::Exit | Op::Ldg { .. } | Op::Stg { .. } => {
            encode_flow_or_memory(&mut w, instr.op, index);
        }
        Op::Mov { .. }
        | Op::Iadd3 { .. }
        | Op::Lop3 { .. }
        | Op::Fadd { .. }
        | Op::Fmul { .. }
        | Op::Ffma { .. }
        | Op::Isetp { .. } => encode_alu_op(&mut w, instr.op),
    }
    w.pred(12, 15, instr.guard);
    let c = instr.ctl;
    w.set(105, 4, u64::from(c.delay));
    w.flag(109, c.yield_);
    w.set(110, 3, u64::from(c.write_sb.unwrap_or(7)));
    w.set(113, 3, u64::from(c.read_sb.unwrap_or(7)));
    w.set(116, 6, u64::from(c.wait));
    w.0
}

/// Encodes the ALU instructions.
fn encode_alu_op(w: &mut Word, op: Op) {
    match op {
        Op::Mov { d, b } => {
            w.alu(0x002, Some(d), None, (b, None));
            w.set(72, 4, 0xF);
        }
        Op::Iadd3 { d, a, b, c } => {
            w.alu(0x010, Some(d), Some(a), (b, Some(c)));
            w.false_pred(87, 90);
            w.false_pred(77, 80);
            w.set(81, 3, u64::from(PT));
            w.set(84, 3, u64::from(PT));
        }
        Op::Lop3 { d, a, b, c, lut } => {
            w.alu(0x012, Some(d), Some(a), (b, Some(c)));
            w.set(72, 8, u64::from(lut));
            w.flag(80, false);
            w.set(81, 3, u64::from(PT));
            w.false_pred(87, 90);
        }
        Op::Fadd { d, a, b, rnd } => {
            match b {
                Operand::Reg(_) => w.alu(0x021, Some(d), Some(a), (b, None)),
                // An immediate addend uses form 2: immediate in bits 32–63, `src2` = `RZ`.
                Operand::Imm(imm) => {
                    w.alu(0x021, Some(d), Some(a), (Operand::Imm(imm), Some(RZ)));
                    w.set(9, 3, 2);
                }
            }
            float_mods(w, rnd);
        }
        Op::Fmul { d, a, b, rnd } => {
            w.alu(0x020, Some(d), Some(a), (b, Some(RZ)));
            w.flag(76, false);
            float_mods(w, rnd);
            w.set(84, 3, 4);
        }
        Op::Ffma { d, a, b, c, rnd } => {
            w.alu(0x023, Some(d), Some(a), (b, Some(c)));
            w.flag(76, false);
            float_mods(w, rnd);
        }
        Op::Isetp {
            p,
            cmp,
            signed,
            a,
            b,
        } => {
            w.alu(0x00C, None, Some(a), (b, None));
            w.pred(68, 71, Pred::TRUE);
            w.flag(72, false);
            w.flag(73, signed);
            w.set(74, 2, 0); // AND with the accumulator
            w.set(76, 3, cmp_bits(cmp));
            w.set(81, 3, u64::from(p));
            w.set(84, 3, u64::from(PT));
            w.pred(87, 90, Pred::TRUE);
        }
        Op::Bra { .. } | Op::Exit | Op::Ldg { .. } | Op::Stg { .. } => {
            unreachable!("not an ALU instruction")
        }
    }
}

/// Encodes branches, `EXIT` and global memory accesses.
fn encode_flow_or_memory(w: &mut Word, op: Op, index: usize) {
    match op {
        Op::Bra { target } => {
            w.set(0, 12, 0x947);
            w.flag(32, false);
            // The guard predicate decides; the condition field reads `PT`.
            w.pred(87, 90, Pred::TRUE);
            w.set(34, 48, branch_offset(index, target));
        }
        Op::Exit => {
            w.set(0, 12, 0x94D);
            w.set(87, 3, u64::from(PT));
        }
        Op::Ldg { d, a, offset } => {
            w.global_access(0x981, a, offset, 32);
            w.set(16, 8, u64::from(d));
            w.set(64, 3, 0); // predicate input `PT`, stored as 7 - index
            w.flag(67, false);
            w.set(81, 3, u64::from(PT));
        }
        Op::Stg { a, v, offset } => {
            w.global_access(0x986, a, offset, 64);
            w.set(32, 8, u64::from(v));
        }
        Op::Mov { .. }
        | Op::Iadd3 { .. }
        | Op::Lop3 { .. }
        | Op::Fadd { .. }
        | Op::Fmul { .. }
        | Op::Ffma { .. }
        | Op::Isetp { .. } => {
            unreachable!("not a flow or memory instruction")
        }
    }
}

/// The 48-bit branch field: target minus the next instruction, in 32-bit words.
fn branch_offset(index: usize, target: usize) -> u64 {
    let words = |i: usize| i64::try_from(i * 4).expect("program fits i64");
    let rel = words(target) - words(index) - 4;
    rel.cast_unsigned() & ((1 << 48) - 1)
}

#[cfg(test)]
mod tests {
    use super::{Control, Instr, Op, Operand, PT, Pred, RZ, Round, encode};

    fn field(w: [u32; 4], lo: usize, width: usize) -> u64 {
        (0..width).fold(0, |acc, i| {
            let bit = lo + i;
            acc | (u64::from((w[bit / 32] >> (bit % 32)) & 1) << i)
        })
    }

    fn plain(op: Op) -> Instr {
        Instr {
            op,
            guard: Pred::TRUE,
            ctl: Control::DEFAULT,
        }
    }

    #[test]
    fn alu_register_form_places_every_operand() {
        let w = encode(
            &plain(Op::Ffma {
                d: 1,
                a: 2,
                b: Operand::Reg(3),
                c: 4,
                rnd: Round::Rz,
            }),
            0,
        );
        assert_eq!(field(w, 0, 9), 0x023);
        assert_eq!(field(w, 9, 3), 1);
        assert_eq!(field(w, 16, 8), 1);
        assert_eq!(field(w, 24, 8), 2);
        assert_eq!(field(w, 32, 8), 3);
        assert_eq!(field(w, 64, 8), 4);
        assert_eq!(field(w, 78, 2), 3);
        assert_eq!(field(w, 12, 3), u64::from(PT));
    }

    #[test]
    fn immediate_second_operand_uses_form_four() {
        let w = encode(
            &plain(Op::Mov {
                d: 5,
                b: Operand::Imm(0xDEAD_BEEF),
            }),
            0,
        );
        assert_eq!(field(w, 0, 9), 0x002);
        assert_eq!(field(w, 9, 3), 4);
        assert_eq!(field(w, 32, 32), 0xDEAD_BEEF);
        assert_eq!(field(w, 72, 4), 0xF);
    }

    #[test]
    fn fadd_with_an_immediate_uses_form_two_and_rz() {
        let w = encode(
            &plain(Op::Fadd {
                d: 0,
                a: 1,
                b: Operand::Imm(1.5f32.to_bits()),
                rnd: Round::Rn,
            }),
            0,
        );
        assert_eq!(field(w, 9, 3), 2);
        assert_eq!(field(w, 32, 32), u64::from(1.5f32.to_bits()));
        assert_eq!(field(w, 64, 8), u64::from(RZ));
    }

    #[test]
    fn control_fields_land_in_bits_105_to_121() {
        let mut i = plain(Op::Exit);
        i.ctl = Control {
            delay: 9,
            write_sb: Some(2),
            read_sb: None,
            wait: 0b10_0001,
            yield_: true,
        };
        let w = encode(&i, 0);
        assert_eq!(field(w, 0, 12), 0x94D);
        assert_eq!(field(w, 105, 4), 9);
        assert_eq!(field(w, 109, 1), 1);
        assert_eq!(field(w, 110, 3), 2);
        assert_eq!(field(w, 113, 3), 7);
        assert_eq!(field(w, 116, 6), 0b10_0001);
    }

    #[test]
    fn branch_offset_is_relative_to_the_next_instruction_in_words() {
        let forward = encode(&plain(Op::Bra { target: 5 }), 2);
        assert_eq!(field(forward, 34, 48), 8);
        let backward = encode(&plain(Op::Bra { target: 0 }), 3);
        assert_eq!(field(backward, 34, 48), (1u64 << 48) - 16);
    }

    #[test]
    fn negated_guard_sets_bit_15() {
        let mut i = plain(Op::Exit);
        i.guard = Pred { reg: 0, not: true };
        let w = encode(&i, 0);
        assert_eq!(field(w, 12, 3), 0);
        assert_eq!(field(w, 15, 1), 1);
    }

    #[test]
    fn global_store_places_address_data_and_offset() {
        let w = encode(
            &plain(Op::Stg {
                a: 2,
                v: 7,
                offset: -16,
            }),
            0,
        );
        assert_eq!(field(w, 0, 12), 0x986);
        assert_eq!(field(w, 24, 8), 2);
        assert_eq!(field(w, 32, 8), 7);
        assert_eq!(field(w, 40, 24), 0x00FF_FFF0);
        assert_eq!(field(w, 73, 3), 4);
        assert_eq!(field(w, 90, 1), 1);
    }
}
