// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Minimal SPIR-V 1.0 emitter for the two test shaders, written from the public Khronos SPIR-V
//! specification (instruction layout: word count in the high 16 bits, opcode in the low 16).
//!
//! - Vertex: writes a `Position` whose `x` is a constant derived from `seed`.
//! - Fragment: starts from `FragCoord.x` and `FragCoord.y` and applies `ops` steps of
//!   `x[n+1] = x[n] * x[n-1] + k`. The recurrence is non-affine, so a driver cannot fold the
//!   chain into one operation. `ops` scales the program size.
//! - `seed` changes the constants, so the harness gets programs that no driver cache has seen:
//!   a vertex shader per run and a fragment shader per sample.

// Opcodes and enumerants (SPIR-V specification, "Binary Form").
const OP_CAPABILITY: u16 = 17;
const OP_MEMORY_MODEL: u16 = 14;
const OP_ENTRY_POINT: u16 = 15;
const OP_EXECUTION_MODE: u16 = 16;
const OP_DECORATE: u16 = 71;
const OP_TYPE_VOID: u16 = 19;
const OP_TYPE_FUNCTION: u16 = 33;
const OP_TYPE_FLOAT: u16 = 22;
const OP_TYPE_VECTOR: u16 = 23;
const OP_TYPE_POINTER: u16 = 32;
const OP_CONSTANT: u16 = 43;
const OP_CONSTANT_COMPOSITE: u16 = 44;
const OP_VARIABLE: u16 = 59;
const OP_FUNCTION: u16 = 54;
const OP_LABEL: u16 = 248;
const OP_LOAD: u16 = 61;
const OP_STORE: u16 = 62;
const OP_COMPOSITE_EXTRACT: u16 = 81;
const OP_COMPOSITE_CONSTRUCT: u16 = 80;
const OP_FMUL: u16 = 133;
const OP_FADD: u16 = 129;
const OP_RETURN: u16 = 253;
const OP_FUNCTION_END: u16 = 56;

const MAGIC: u32 = 0x0723_0203;
const VERSION_1_0: u32 = 0x0001_0000;
const CAPABILITY_SHADER: u32 = 1;
const ADDRESSING_LOGICAL: u32 = 0;
const MEMORY_MODEL_GLSL450: u32 = 1;
const EXECUTION_MODEL_VERTEX: u32 = 0;
const EXECUTION_MODEL_FRAGMENT: u32 = 4;
const EXECUTION_MODE_ORIGIN_UPPER_LEFT: u32 = 7;
const DECORATION_BUILTIN: u32 = 11;
const DECORATION_LOCATION: u32 = 30;
const BUILTIN_POSITION: u32 = 0;
const BUILTIN_FRAG_COORD: u32 = 15;
const STORAGE_INPUT: u32 = 1;
const STORAGE_OUTPUT: u32 = 3;
const FUNCTION_CONTROL_NONE: u32 = 0;

/// Number of distinct multiply/add constants cycled through by the fragment shader.
const CONSTANTS: u32 = 8;

/// Accumulates instructions and hands out result ids.
struct Module {
    words: Vec<u32>,
    next_id: u32,
}

impl Module {
    fn new() -> Self {
        // Header; word 3 (id bound) is patched in `finish`.
        Self {
            words: vec![MAGIC, VERSION_1_0, 0, 0, 0],
            next_id: 1,
        }
    }

    fn id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn inst(&mut self, opcode: u16, operands: &[u32]) {
        let count = u16::try_from(operands.len() + 1)
            .expect("invariant: a SPIR-V instruction has at most 65535 words");
        self.words
            .push((u32::from(count) << 16) | u32::from(opcode));
        self.words.extend_from_slice(operands);
    }

    fn finish(mut self) -> Vec<u32> {
        self.words[3] = self.next_id;
        self.words
    }
}

/// A nul-terminated UTF-8 literal packed into little-endian words.
fn literal(s: &str) -> Vec<u32> {
    let mut bytes = s.as_bytes().to_vec();
    bytes.push(0);
    bytes.resize(bytes.len().div_ceil(4) * 4, 0);
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect()
}

fn entry_point(m: &mut Module, model: u32, function: u32, interface: &[u32]) {
    let mut operands = vec![model, function];
    operands.extend(literal("main"));
    operands.extend_from_slice(interface);
    m.inst(OP_ENTRY_POINT, &operands);
}

/// `SplitMix64` / Fibonacci-hashing increment (2^64 / golden ratio), used to spread seeds.
const SEED_MIX: u64 = 0x9E37_79B9_7F4A_7C15;
/// Bit pattern of `1.0f32`: sign 0, exponent 127, mantissa 0.
const ONE_F32_BITS: u32 = 0x3F80_0000;
/// Shift that keeps the top 23 bits of a `u64`, the width of an `f32` mantissa.
const MANTISSA_SHIFT: u32 = 64 - 23;

/// A finite constant in `[1, 2)` that depends on `seed` and `index`.
fn constant(seed: u64, index: u32) -> f32 {
    let mixed = seed.wrapping_add(u64::from(index)).wrapping_mul(SEED_MIX);
    let mantissa = u32::try_from(mixed >> MANTISSA_SHIFT)
        .expect("invariant: a 64-bit value shifted right by 41 fits in 23 bits");
    f32::from_bits(ONE_F32_BITS | mantissa)
}

/// Vertex shader writing a position whose `x` depends on `seed`.
pub(crate) fn vertex_shader(seed: u64) -> Vec<u32> {
    let mut m = Module::new();
    let (void, func_ty, float, vec4, out_ptr, position) =
        (m.id(), m.id(), m.id(), m.id(), m.id(), m.id());
    let (x, zero, one, value, main, label) = (m.id(), m.id(), m.id(), m.id(), m.id(), m.id());
    m.inst(OP_CAPABILITY, &[CAPABILITY_SHADER]);
    m.inst(OP_MEMORY_MODEL, &[ADDRESSING_LOGICAL, MEMORY_MODEL_GLSL450]);
    entry_point(&mut m, EXECUTION_MODEL_VERTEX, main, &[position]);
    m.inst(
        OP_DECORATE,
        &[position, DECORATION_BUILTIN, BUILTIN_POSITION],
    );
    m.inst(OP_TYPE_VOID, &[void]);
    m.inst(OP_TYPE_FUNCTION, &[func_ty, void]);
    m.inst(OP_TYPE_FLOAT, &[float, 32]);
    m.inst(OP_TYPE_VECTOR, &[vec4, float, 4]);
    m.inst(OP_TYPE_POINTER, &[out_ptr, STORAGE_OUTPUT, vec4]);
    m.inst(OP_VARIABLE, &[out_ptr, position, STORAGE_OUTPUT]);
    m.inst(OP_CONSTANT, &[float, x, constant(seed, 0).to_bits()]);
    m.inst(OP_CONSTANT, &[float, zero, 0.0f32.to_bits()]);
    m.inst(OP_CONSTANT, &[float, one, 1.0f32.to_bits()]);
    m.inst(OP_CONSTANT_COMPOSITE, &[vec4, value, x, zero, zero, one]);
    m.inst(OP_FUNCTION, &[void, main, FUNCTION_CONTROL_NONE, func_ty]);
    m.inst(OP_LABEL, &[label]);
    m.inst(OP_STORE, &[position, value]);
    m.inst(OP_RETURN, &[]);
    m.inst(OP_FUNCTION_END, &[]);
    m.finish()
}

/// Declarations shared by the fragment shader: returns `(float, vec4, frag_coord, color, one,
/// constants)` ids after emitting the header, types, variables and constants.
fn fragment_prologue(m: &mut Module, seed: u64) -> (u32, u32, u32, u32, u32, Vec<u32>) {
    let (void, func_ty, float, vec4, in_ptr, out_ptr) =
        (m.id(), m.id(), m.id(), m.id(), m.id(), m.id());
    let (frag_coord, color, one, main) = (m.id(), m.id(), m.id(), m.id());
    let constants: Vec<u32> = (0..CONSTANTS).map(|_| m.id()).collect();
    m.inst(OP_CAPABILITY, &[CAPABILITY_SHADER]);
    m.inst(OP_MEMORY_MODEL, &[ADDRESSING_LOGICAL, MEMORY_MODEL_GLSL450]);
    entry_point(m, EXECUTION_MODEL_FRAGMENT, main, &[frag_coord, color]);
    m.inst(OP_EXECUTION_MODE, &[main, EXECUTION_MODE_ORIGIN_UPPER_LEFT]);
    m.inst(
        OP_DECORATE,
        &[frag_coord, DECORATION_BUILTIN, BUILTIN_FRAG_COORD],
    );
    m.inst(OP_DECORATE, &[color, DECORATION_LOCATION, 0]);
    m.inst(OP_TYPE_VOID, &[void]);
    m.inst(OP_TYPE_FUNCTION, &[func_ty, void]);
    m.inst(OP_TYPE_FLOAT, &[float, 32]);
    m.inst(OP_TYPE_VECTOR, &[vec4, float, 4]);
    m.inst(OP_TYPE_POINTER, &[in_ptr, STORAGE_INPUT, vec4]);
    m.inst(OP_TYPE_POINTER, &[out_ptr, STORAGE_OUTPUT, vec4]);
    m.inst(OP_VARIABLE, &[in_ptr, frag_coord, STORAGE_INPUT]);
    m.inst(OP_VARIABLE, &[out_ptr, color, STORAGE_OUTPUT]);
    m.inst(OP_CONSTANT, &[float, one, 1.0f32.to_bits()]);
    for (index, &id) in (0u32..).zip(&constants) {
        m.inst(OP_CONSTANT, &[float, id, constant(seed, index).to_bits()]);
    }
    m.inst(OP_FUNCTION, &[void, main, FUNCTION_CONTROL_NONE, func_ty]);
    (float, vec4, frag_coord, color, one, constants)
}

/// Fragment shader with `ops` steps of a non-affine recurrence; distinct for each `seed`.
pub(crate) fn fragment_shader(ops: u32, seed: u64) -> Vec<u32> {
    let mut m = Module::new();
    let (float, vec4, frag_coord, color, one, constants) = fragment_prologue(&mut m, seed);
    let label = m.id();
    m.inst(OP_LABEL, &[label]);
    let coord = m.id();
    m.inst(OP_LOAD, &[vec4, coord, frag_coord]);
    let (mut prev, mut x) = (m.id(), m.id());
    m.inst(OP_COMPOSITE_EXTRACT, &[float, prev, coord, 0]);
    m.inst(OP_COMPOSITE_EXTRACT, &[float, x, coord, 1]);
    for i in 0..ops {
        let k = constants[(i % CONSTANTS) as usize];
        let product = m.id();
        m.inst(OP_FMUL, &[float, product, x, prev]);
        let sum = m.id();
        m.inst(OP_FADD, &[float, sum, product, k]);
        prev = x;
        x = sum;
    }
    let result = m.id();
    m.inst(OP_COMPOSITE_CONSTRUCT, &[vec4, result, x, prev, x, one]);
    m.inst(OP_STORE, &[color, result]);
    m.inst(OP_RETURN, &[]);
    m.inst(OP_FUNCTION_END, &[]);
    m.finish()
}

#[cfg(test)]
mod tests {
    use super::{MAGIC, constant, fragment_shader, literal, vertex_shader};

    /// Walks the instruction stream; returns (opcode, word count) pairs.
    fn instructions(words: &[u32]) -> Vec<(u32, usize)> {
        let mut out = Vec::new();
        let mut i = 5;
        while i < words.len() {
            let count = (words[i] >> 16) as usize;
            assert!(
                count > 0 && i + count <= words.len(),
                "bad word count at {i}"
            );
            out.push((words[i] & 0xFFFF, count));
            i += count;
        }
        out
    }

    #[test]
    fn modules_have_a_valid_header_and_well_formed_instructions() {
        for module in [vertex_shader(1), fragment_shader(4, 1)] {
            assert_eq!(module[0], MAGIC);
            assert_eq!(module[1], 0x0001_0000);
            assert!(module[3] > 1, "id bound");
            assert!(instructions(&module).len() > 5);
        }
    }

    #[test]
    fn fragment_size_grows_with_ops_and_differs_per_seed() {
        let small = fragment_shader(16, 7);
        let large = fragment_shader(1024, 7);
        assert_eq!(large.len() - small.len(), (1024 - 16) * 2 * 5);
        assert_ne!(fragment_shader(16, 7), fragment_shader(16, 8));
        assert_ne!(vertex_shader(7), vertex_shader(8));
        assert_eq!(fragment_shader(16, 7), small);
    }

    #[test]
    fn constants_are_finite_and_in_range() {
        for seed in 0..1000 {
            let c = constant(seed, 3);
            assert!(c.is_finite() && (1.0..2.0).contains(&c));
        }
    }

    #[test]
    fn literals_are_nul_terminated_and_padded() {
        assert_eq!(literal("main"), vec![u32::from_le_bytes(*b"main"), 0]);
        assert_eq!(literal("abc").len(), 1);
    }
}
