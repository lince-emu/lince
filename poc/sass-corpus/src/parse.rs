// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Text form of corpus programs.
//!
//! ```text
//! # comment
//! .word 0x1000 0x3fc00000 0x40000000    # initial memory: address, then 32-bit words
//! loop:                                  # label
//!     @!P0 FADD.RN R1, R2, 1.5f ; d=4 wr=0 wait=0x1 yield
//! ```
//!
//! Registers are `R0`–`R254` and `RZ`; predicates `P0`–`P6` and `PT`. Operands that may be an
//! immediate accept hexadecimal (`0x…`), decimal (`-3`) or `f32` (`1.5f`) literals. Control
//! fields after `;` default to [`Control::DEFAULT`].

use std::collections::BTreeMap;

use crate::isa::{Cmp, Control, Instr, Op, Operand, PT, Pred, RZ, Round};

/// A parsed program: its instructions and its initial global memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    /// Instructions in order; branch targets are indices into this list.
    pub instrs: Vec<Instr>,
    /// Initial memory, by byte address (each entry is one aligned 32-bit word).
    pub memory: BTreeMap<u64, u32>,
}

/// A parse error with its 1-based line number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Line of the error.
    pub line: usize,
    /// What is wrong.
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

type Result<T> = std::result::Result<T, String>;

/// Parses a whole program.
pub fn parse(text: &str) -> std::result::Result<Program, ParseError> {
    let lines: Vec<(usize, &str)> = text
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.split('#').next().unwrap_or("").trim()))
        .filter(|(_, l)| !l.is_empty())
        .collect();
    // First pass: label positions.
    let mut labels = BTreeMap::new();
    let mut count = 0usize;
    for &(line, l) in &lines {
        if let Some(name) = l.strip_suffix(':') {
            if labels.insert(name.to_owned(), count).is_some() {
                return Err(error(line, format!("label `{name}` defined twice")));
            }
        } else if !l.starts_with(".word") {
            count += 1;
        }
    }
    let mut program = Program {
        instrs: Vec::new(),
        memory: BTreeMap::new(),
    };
    for (line, l) in lines {
        let result = if l.ends_with(':') {
            Ok(())
        } else if let Some(rest) = l.strip_prefix(".word") {
            words(rest, &mut program.memory)
        } else {
            instruction(l, &labels).map(|i| program.instrs.push(i))
        };
        result.map_err(|m| error(line, m))?;
    }
    Ok(program)
}

fn error(line: usize, message: String) -> ParseError {
    ParseError { line, message }
}

fn words(rest: &str, memory: &mut BTreeMap<u64, u32>) -> Result<()> {
    let mut it = rest.split_whitespace();
    let base = number(it.next().ok_or(".word needs an address")?)?;
    if base % 4 != 0 {
        return Err(format!("address {base:#x} is not 4-byte aligned"));
    }
    for (i, w) in it.enumerate() {
        let addr = u64::from(base) + 4 * u64::try_from(i).map_err(|_| "too many words")?;
        memory.insert(addr, number(w)?);
    }
    Ok(())
}

fn instruction(l: &str, labels: &BTreeMap<String, usize>) -> Result<Instr> {
    let (body, ctl) = match l.split_once(';') {
        Some((b, c)) => (b.trim(), control(c)?),
        None => (l, Control::DEFAULT),
    };
    let (guard, body) = match body.strip_prefix('@') {
        Some(rest) => {
            let (p, rest) = rest.split_once(' ').ok_or("guard without instruction")?;
            let (not, p) = p.strip_prefix('!').map_or((false, p), |p| (true, p));
            (Pred { reg: pred(p)?, not }, rest.trim())
        }
        None => (Pred::TRUE, body),
    };
    let (mnemonic, args) = body.split_once(' ').unwrap_or((body, ""));
    let args: Vec<&str> = if args.trim().is_empty() {
        Vec::new()
    } else {
        args.split(',').map(str::trim).collect()
    };
    let op = op(mnemonic, &args, labels)?;
    Ok(Instr { op, guard, ctl })
}

fn op(mnemonic: &str, a: &[&str], labels: &BTreeMap<String, usize>) -> Result<Op> {
    let mut parts = mnemonic.split('.');
    let name = parts.next().unwrap_or_default();
    let mods: Vec<&str> = parts.collect();
    let want = |n: usize| -> Result<()> {
        if a.len() == n {
            Ok(())
        } else {
            Err(format!("{name} takes {n} operands, got {}", a.len()))
        }
    };
    Ok(match name {
        "MOV" => {
            want(2)?;
            Op::Mov {
                d: reg(a[0])?,
                b: operand(a[1])?,
            }
        }
        "IADD3" => {
            want(4)?;
            Op::Iadd3 {
                d: reg(a[0])?,
                a: reg(a[1])?,
                b: operand(a[2])?,
                c: reg(a[3])?,
            }
        }
        "LOP3" => {
            want(5)?;
            let lut = u8::try_from(number(a[4])?).map_err(|_| "LUT must fit 8 bits")?;
            Op::Lop3 {
                d: reg(a[0])?,
                a: reg(a[1])?,
                b: operand(a[2])?,
                c: reg(a[3])?,
                lut,
            }
        }
        "FADD" | "FMUL" | "FFMA" | "ISETP" => return float_or_compare(name, &mods, a),
        "BRA" => {
            want(1)?;
            let target = *labels
                .get(a[0])
                .ok_or_else(|| format!("unknown label `{}`", a[0]))?;
            Op::Bra { target }
        }
        "EXIT" => {
            want(0)?;
            Op::Exit
        }
        "LDG" => {
            want(2)?;
            let (r, offset) = address(a[1])?;
            Op::Ldg {
                d: reg(a[0])?,
                a: r,
                offset,
            }
        }
        "STG" => {
            want(2)?;
            let (r, offset) = address(a[0])?;
            Op::Stg {
                a: r,
                v: reg(a[1])?,
                offset,
            }
        }
        _ => return Err(format!("unknown instruction `{mnemonic}`")),
    })
}

/// `FADD`, `FMUL`, `FFMA` and `ISETP`.
fn float_or_compare(name: &str, mods: &[&str], a: &[&str]) -> Result<Op> {
    let want = |n: usize| -> Result<()> {
        if a.len() == n {
            Ok(())
        } else {
            Err(format!("{name} takes {n} operands, got {}", a.len()))
        }
    };
    Ok(match name {
        "FADD" | "FMUL" => {
            want(3)?;
            let (d, x, b, rnd) = (reg(a[0])?, reg(a[1])?, operand(a[2])?, round(mods)?);
            if name == "FADD" {
                Op::Fadd { d, a: x, b, rnd }
            } else {
                Op::Fmul { d, a: x, b, rnd }
            }
        }
        "FFMA" => {
            want(4)?;
            let rnd = round(mods)?;
            Op::Ffma {
                d: reg(a[0])?,
                a: reg(a[1])?,
                b: operand(a[2])?,
                c: reg(a[3])?,
                rnd,
            }
        }
        "ISETP" => {
            want(3)?;
            let (cmp, signed) = compare(mods)?;
            Op::Isetp {
                p: pred(a[0])?,
                cmp,
                signed,
                a: reg(a[1])?,
                b: operand(a[2])?,
            }
        }
        _ => return Err(format!("unknown instruction `{name}`")),
    })
}

fn reg(s: &str) -> Result<u8> {
    if s == "RZ" {
        return Ok(RZ);
    }
    s.strip_prefix('R')
        .and_then(|n| n.parse::<u8>().ok())
        .filter(|&n| n < RZ)
        .ok_or_else(|| format!("bad register `{s}`"))
}

fn pred(s: &str) -> Result<u8> {
    if s == "PT" {
        return Ok(PT);
    }
    s.strip_prefix('P')
        .and_then(|n| n.parse::<u8>().ok())
        .filter(|&n| n < PT)
        .ok_or_else(|| format!("bad predicate `{s}`"))
}

fn operand(s: &str) -> Result<Operand> {
    if s.starts_with('R') {
        return reg(s).map(Operand::Reg);
    }
    // A hexadecimal literal may end in `f` too; only decimal literals carry the float suffix.
    if let Some(f) = s.strip_suffix('f').filter(|_| !s.starts_with("0x")) {
        let v: f32 = f.parse().map_err(|_| format!("bad float `{s}`"))?;
        return Ok(Operand::Imm(v.to_bits()));
    }
    number(s).map(Operand::Imm)
}

/// A 32-bit literal: hexadecimal bits, or a decimal value that fits `i32` or `u32`.
fn number(s: &str) -> Result<u32> {
    if let Some(hex) = s.strip_prefix("0x") {
        return u32::from_str_radix(hex, 16).map_err(|_| format!("bad number `{s}`"));
    }
    s.parse::<u32>()
        .or_else(|_| s.parse::<i32>().map(i32::cast_unsigned))
        .map_err(|_| format!("bad number `{s}`"))
}

/// `[Rn+off]` or `[Rn-off]` or `[Rn]`; `Rn` is the low half of a 64-bit address pair.
fn address(s: &str) -> Result<(u8, i32)> {
    let inner = s
        .strip_prefix('[')
        .and_then(|t| t.strip_suffix(']'))
        .ok_or_else(|| format!("bad address `{s}`"))?;
    let split = inner.find(['+', '-']);
    let (r, off) = split.map_or((inner, "0"), |i| inner.split_at(i));
    let off = off.strip_prefix('+').unwrap_or(off);
    let offset = number(off)?.cast_signed();
    if !(-(1 << 23)..(1 << 23)).contains(&offset) {
        return Err(format!("offset {offset} does not fit 24 bits"));
    }
    Ok((reg(r)?, offset))
}

fn round(mods: &[&str]) -> Result<Round> {
    match mods {
        [] | ["RN"] => Ok(Round::Rn),
        ["RM"] => Ok(Round::Rm),
        ["RP"] => Ok(Round::Rp),
        ["RZ"] => Ok(Round::Rz),
        _ => Err(format!("bad rounding modifiers {mods:?}")),
    }
}

fn compare(mods: &[&str]) -> Result<(Cmp, bool)> {
    let [cmp, ty] = mods else {
        return Err("ISETP needs .<CMP>.<U32|I32>".to_owned());
    };
    let cmp = match *cmp {
        "LT" => Cmp::Lt,
        "EQ" => Cmp::Eq,
        "LE" => Cmp::Le,
        "GT" => Cmp::Gt,
        "NE" => Cmp::Ne,
        "GE" => Cmp::Ge,
        other => return Err(format!("bad comparison `{other}`")),
    };
    let signed = match *ty {
        "U32" => false,
        "I32" => true,
        other => return Err(format!("bad comparison type `{other}`")),
    };
    Ok((cmp, signed))
}

/// `d=N wr=N rd=N wait=MASK yield`.
fn control(s: &str) -> Result<Control> {
    let mut c = Control::DEFAULT;
    for token in s.split_whitespace() {
        let (key, value) = token.split_once('=').unwrap_or((token, ""));
        let small = |max: u32| -> Result<u8> {
            let v = number(value)?;
            u8::try_from(v)
                .ok()
                .filter(|&v| u32::from(v) <= max)
                .ok_or_else(|| format!("{key}={value} is out of range"))
        };
        match key {
            "d" => c.delay = small(15)?,
            "wr" => c.write_sb = Some(small(5)?),
            "rd" => c.read_sb = Some(small(5)?),
            "wait" => c.wait = small(0x3F)?,
            "yield" => c.yield_ = true,
            _ => return Err(format!("unknown control field `{key}`")),
        }
    }
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::isa::{Op, Operand, Pred, Round};

    #[test]
    fn parses_labels_guards_control_and_memory() {
        let p = parse(
            ".word 0x100 1 2\nstart:\n  @!P1 FADD.RZ R1, R2, 1.5f ; d=4 wr=0 wait=0x3 yield\n  BRA start\n",
        )
        .unwrap();
        assert_eq!(p.memory.get(&0x104), Some(&2));
        assert_eq!(p.instrs.len(), 2);
        let i = p.instrs[0];
        assert_eq!(i.guard, Pred { reg: 1, not: true });
        assert_eq!(
            i.op,
            Op::Fadd {
                d: 1,
                a: 2,
                b: Operand::Imm(1.5f32.to_bits()),
                rnd: Round::Rz
            }
        );
        assert_eq!(
            (i.ctl.delay, i.ctl.write_sb, i.ctl.wait, i.ctl.yield_),
            (4, Some(0), 3, true)
        );
        assert_eq!(p.instrs[1].op, Op::Bra { target: 0 });
    }

    #[test]
    fn rejects_unknown_labels_and_bad_registers_with_the_line() {
        let e = parse("EXIT\nBRA nowhere\n").unwrap_err();
        assert_eq!(e.line, 2);
        assert!(parse("MOV R255, 1\n").is_err());
    }

    #[test]
    fn hexadecimal_literals_ending_in_f_are_integers() {
        let p = parse("MOV R0, 0xffffffff\nMOV R1, 2.5f\n").unwrap();
        assert_eq!(
            p.instrs[0].op,
            Op::Mov {
                d: 0,
                b: Operand::Imm(0xFFFF_FFFF)
            }
        );
        assert_eq!(
            p.instrs[1].op,
            Op::Mov {
                d: 1,
                b: Operand::Imm(2.5f32.to_bits())
            }
        );
    }

    #[test]
    fn negative_address_offsets_parse() {
        let p = parse("LDG.E.32 R4, [R2-16]\n").unwrap();
        assert_eq!(
            p.instrs[0].op,
            Op::Ldg {
                d: 4,
                a: 2,
                offset: -16
            }
        );
    }
}
