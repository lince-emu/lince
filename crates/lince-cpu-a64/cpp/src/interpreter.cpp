// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Tier A interpreter (ADR-0005): the reference semantics of every decoded form, executed one
// instruction at a time. Semantics follow the pseudocode of the Arm Architecture Reference
// Manual for A-profile (public, LEGAL-11). Built without exceptions or RTTI; no threads, I/O or
// mutable globals (ARCH-51, ARCH-52).

#include "machine.hpp"

#include "lince-cpu-a64.h"

#include <array>
#include <bit>
#include <cstddef>
#include <cstdint>
#include <optional>

namespace lince::a64 {
namespace {

// AddWithCarry() of the Arm ARM: the sum at width `w` and its NZCV flags.
struct Sum {
  uint64_t value;
  uint32_t nzcv;
};

Sum add_with_carry(uint64_t a, uint64_t b, bool carry_in, Width w) {
  const bool sf = w == Width::X;
  const uint64_t mask = sf ? ~uint64_t{0} : kWMask;
  const unsigned bits = sf ? kXBits : kWBits;
  a &= mask;
  b &= mask;
  const uint64_t partial = a + b;
  const uint64_t result = (partial + (carry_in ? 1U : 0U)) & mask;
  bool carry = false;
  if (sf) {
    carry = partial < a || (carry_in && partial + 1 == 0);
  } else {
    carry = ((a + b + (carry_in ? 1U : 0U)) >> kWBits) != 0;
  }
  const uint64_t sign = uint64_t{1} << (bits - 1);
  const bool overflow = (((a ^ result) & (b ^ result)) & sign) != 0;
  uint32_t nzcv = 0;
  nzcv |= (result & sign) != 0 ? kFlagN : 0;
  nzcv |= result == 0 ? kFlagZ : 0;
  nzcv |= carry ? kFlagC : 0;
  nzcv |= overflow ? kFlagV : 0;
  return {.value = result, .nzcv = nzcv};
}

// The condition tested by bits 3..1 of a condition code; bit 0 inverts it (except for 0b1111).
enum class Condition : uint8_t { EQ, CS, MI, VS, HI, GE, GT, AL };

// ConditionHolds() of the Arm ARM.
bool condition_holds(uint32_t cond, const Core &core) {
  const uint32_t nzcv = core.nzcv();
  const bool n = (nzcv & kFlagN) != 0;
  const bool z = (nzcv & kFlagZ) != 0;
  const bool c = (nzcv & kFlagC) != 0;
  const bool v = (nzcv & kFlagV) != 0;
  bool result = true;
  switch (static_cast<Condition>(cond >> 1U)) {
  case Condition::EQ:
    result = z;
    break;
  case Condition::CS:
    result = c;
    break;
  case Condition::MI:
    result = n;
    break;
  case Condition::VS:
    result = v;
    break;
  case Condition::HI:
    result = c && !z;
    break;
  case Condition::GE:
    result = n == v;
    break;
  case Condition::GT:
    result = n == v && !z;
    break;
  case Condition::AL:
    break;
  }
  constexpr uint32_t kAlways = 0b1111;
  if ((cond & 1U) != 0 && cond != kAlways) {
    result = !result;
  }
  return result;
}

Outcome branch_relative(Core &core, uint64_t offset) {
  core.set_pc(core.pc() + offset);
  return Outcome::Jumped;
}

Outcome exec_branch(Core &core, Insn insn, uint32_t word) {
  const uint64_t imm26 =
      sign_extend<kImm26.width + kOffsetShift>(uint64_t{field(word, kImm26)} << kOffsetShift);
  const uint64_t imm19 =
      sign_extend<kImm19.width + kOffsetShift>(uint64_t{field(word, kImm19)} << kOffsetShift);
  const uint64_t imm14 =
      sign_extend<kImm14.width + kOffsetShift>(uint64_t{field(word, kImm14)} << kOffsetShift);
  const Reg rt = reg(word, kRt);
  const uint64_t value = width(word) == Width::X ? core.xr(rt) : core.xr(rt) & kWMask;
  const unsigned bit = (field(word, kB5) << kB5Shift) | field(word, kB40);
  switch (insn) {
  case Insn::B:
    return branch_relative(core, imm26);
  case Insn::BL:
    core.set_xr(kLinkRegister, core.pc() + kInsnBytes, Width::X);
    return branch_relative(core, imm26);
  case Insn::B_COND:
    return condition_holds(field(word, kCond), core) ? branch_relative(core, imm19) : Outcome::Next;
  case Insn::CBZ:
    return value == 0 ? branch_relative(core, imm19) : Outcome::Next;
  case Insn::CBNZ:
    return value != 0 ? branch_relative(core, imm19) : Outcome::Next;
  case Insn::TBZ:
    return ((core.xr(rt) >> bit) & 1U) == 0 ? branch_relative(core, imm14) : Outcome::Next;
  case Insn::TBNZ:
    return ((core.xr(rt) >> bit) & 1U) != 0 ? branch_relative(core, imm14) : Outcome::Next;
  case Insn::RET:
    core.set_pc(core.xr(reg(word, kRn)));
    return Outcome::Jumped;
  default:
    return Outcome::Unimplemented;
  }
}

Outcome exec_add_sub_imm(Core &core, Insn insn, uint32_t word) {
  const Width w = width(word);
  const uint64_t imm = uint64_t{field(word, kImm12)} << (field(word, kSh) != 0 ? kImm12Shift : 0);
  const uint64_t operand = core.xs(reg(word, kRn));
  const bool subtract = insn == Insn::SUB_IMM || insn == Insn::SUBS_IMM;
  const Sum sum = add_with_carry(operand, subtract ? ~imm : imm, subtract, w);
  if (insn == Insn::ADDS_IMM || insn == Insn::SUBS_IMM) {
    core.set_nzcv(sum.nzcv);
    core.set_xr(reg(word, kRd), sum.value, w);
  } else {
    core.set_xs(reg(word, kRd), sum.value, w);
  }
  return Outcome::Next;
}

Outcome exec_move_wide(Core &core, Insn insn, uint32_t word) {
  const unsigned shift = field(word, kHw) * kHwScale;
  const uint64_t imm = uint64_t{field(word, kImm16)} << shift;
  const Reg rd = reg(word, kRd);
  constexpr uint64_t kHalfword = 0xffff;
  uint64_t value = imm;
  if (insn == Insn::MOVK) {
    value = (core.xr(rd) & ~(kHalfword << shift)) | imm;
  } else if (insn == Insn::MOVN) {
    value = ~imm;
  }
  core.set_xr(rd, value, width(word));
  return Outcome::Next;
}

// ADR: pc + imm; ADRP: the 4 KiB page of pc + imm * 4096.
Outcome exec_pc_relative(Core &core, Insn insn, uint32_t word) {
  const uint64_t imm = (uint64_t{field(word, kImm19)} << kImmLo.width) | field(word, kImmLo);
  constexpr unsigned kImmBits = kImm19.width + kImmLo.width;
  uint64_t address = core.pc() + sign_extend<kImmBits>(imm);
  if (insn == Insn::ADRP) {
    const uint64_t page = core.pc() & ~((uint64_t{1} << kPageBits) - 1);
    address = page + sign_extend<kImmBits + kPageBits>(imm << kPageBits);
  }
  core.set_xr(reg(word, kRd), address, Width::X);
  return Outcome::Next;
}

// The low `bits` bits set; `bits` is at most 64.
constexpr uint64_t ones(unsigned bits) {
  return bits >= kXBits ? ~uint64_t{0} : (uint64_t{1} << bits) - 1;
}

constexpr unsigned bits_of(Width w) { return w == Width::X ? kXBits : kWBits; }

// `value` rotated right by `amount` within its low `bits` bits; amount < bits.
constexpr uint64_t rotate_right(uint64_t value, unsigned amount, unsigned bits) {
  if (amount == 0) {
    return value & ones(bits);
  }
  return ((value >> amount) | (value << (bits - amount))) & ones(bits);
}

// DecodeBitMasks() of the Arm ARM: the masks a bitmask immediate or bitfield form encodes.
struct BitMasks {
  uint64_t wmask;
  uint64_t tmask;
};

constexpr unsigned kImmsBits = 6;

std::optional<BitMasks> decode_bit_masks(uint32_t word, bool immediate, Width w) {
  const uint32_t imms = field(word, kImms);
  const auto combined =
      static_cast<uint32_t>((field(word, kBitN) << kImmsBits) | (~imms & ones(kImmsBits)));
  if (combined < 2) {
    return std::nullopt; // len < 1: reserved
  }
  const auto len = static_cast<unsigned>(std::bit_width(combined) - 1);
  const unsigned esize = 1U << len;
  const auto levels = static_cast<uint32_t>(ones(len));
  if (esize > bits_of(w) || (immediate && (imms & levels) == levels)) {
    return std::nullopt;
  }
  const uint32_t s = imms & levels;
  const uint32_t r = field(word, kImmr) & levels;
  const uint32_t d = (s - r) & levels;
  const uint64_t welem = rotate_right(ones(s + 1), r, esize);
  const uint64_t telem = ones(d + 1);
  BitMasks masks{.wmask = 0, .tmask = 0};
  for (unsigned at = 0; at < bits_of(w); at += esize) {
    masks.wmask |= welem << at;
    masks.tmask |= telem << at;
  }
  return masks;
}

// AND/ORR (immediate): Rn is the zero register, Rd may be SP.
Outcome exec_logical_imm(Core &core, Insn insn, uint32_t word) {
  const Width w = width(word);
  const std::optional<BitMasks> masks = decode_bit_masks(word, true, w);
  if (!masks) {
    return Outcome::Undefined;
  }
  const uint64_t operand = core.xr(reg(word, kRn));
  const uint64_t result = insn == Insn::AND_IMM ? operand & masks->wmask : operand | masks->wmask;
  core.set_xs(reg(word, kRd), result, w);
  return Outcome::Next;
}

// UBFM (LSL, LSR, UBFX and UBFIZ are aliases).
Outcome exec_ubfm(Core &core, uint32_t word) {
  const Width w = width(word);
  const std::optional<BitMasks> masks = decode_bit_masks(word, false, w);
  if (!masks) {
    return Outcome::Undefined;
  }
  const uint64_t source = core.xr(reg(word, kRn)) & ones(bits_of(w));
  const uint64_t bottom = rotate_right(source, field(word, kImmr), bits_of(w)) & masks->wmask;
  core.set_xr(reg(word, kRd), bottom & masks->tmask, w);
  return Outcome::Next;
}

// ShiftReg() of the Arm ARM: LSL, LSR, ASR or ROR by `amount` < the operand width.
enum class Shift : uint8_t { LSL, LSR, ASR, ROR };

uint64_t shift_register(uint64_t value, Shift type, unsigned amount, Width w) {
  const unsigned bits = bits_of(w);
  value &= ones(bits);
  switch (type) {
  case Shift::LSL:
    return (value << amount) & ones(bits);
  case Shift::LSR:
    return value >> amount;
  case Shift::ASR: {
    const bool negative = (value >> (bits - 1)) != 0;
    const uint64_t fill = negative ? ones(bits) & ~(ones(bits) >> amount) : 0;
    return (value >> amount) | fill;
  }
  case Shift::ROR:
    return rotate_right(value, amount, bits);
  }
  return value;
}

uint64_t shifted_rm(const Core &core, uint32_t word) {
  return shift_register(core.xr(reg(word, kRm)), static_cast<Shift>(field(word, kShift)),
                        field(word, kImm6), width(word));
}

// ADD, SUB and SUBS (shifted register): every register operand 31 is the zero register.
Outcome exec_add_sub_reg(Core &core, Insn insn, uint32_t word) {
  const Width w = width(word);
  const uint64_t operand = shifted_rm(core, word);
  const bool subtract = insn != Insn::ADD_REG;
  const Sum sum =
      add_with_carry(core.xr(reg(word, kRn)), subtract ? ~operand : operand, subtract, w);
  if (insn == Insn::SUBS_REG) {
    core.set_nzcv(sum.nzcv);
  }
  core.set_xr(reg(word, kRd), sum.value, w);
  return Outcome::Next;
}

// ORR, ORN and EOR (shifted register).
Outcome exec_logical_reg(Core &core, Insn insn, uint32_t word) {
  uint64_t operand = shifted_rm(core, word);
  if (field(word, kInvert) != 0) {
    operand = ~operand;
  }
  const uint64_t rn = core.xr(reg(word, kRn));
  const uint64_t result = insn == Insn::EOR_REG ? rn ^ operand : rn | operand;
  core.set_xr(reg(word, kRd), result, width(word));
  return Outcome::Next;
}

// CSEL and CSINC (CSET and CINC are aliases): Rn if the condition holds, else Rm (plus one).
Outcome exec_cond_select(Core &core, Insn insn, uint32_t word) {
  constexpr Field kSelectCond{.lsb = 12, .width = 4};
  uint64_t value = core.xr(reg(word, kRn));
  if (!condition_holds(field(word, kSelectCond), core)) {
    value = core.xr(reg(word, kRm)) + (insn == Insn::CSINC ? 1U : 0U);
  }
  core.set_xr(reg(word, kRd), value, width(word));
  return Outcome::Next;
}

// LSRV (LSR register): Rn shifted right by Rm modulo the operand width.
Outcome exec_shift_variable(Core &core, uint32_t word) {
  const Width w = width(word);
  const auto amount = static_cast<unsigned>(core.xr(reg(word, kRm)) % bits_of(w));
  core.set_xr(reg(word, kRd), shift_register(core.xr(reg(word, kRn)), Shift::LSR, amount, w), w);
  return Outcome::Next;
}

// MRS: only TPIDRRO_EL0 (op0 3, op1 3, CRn 13, CRm 0, op2 3), the thread pointer B2 programs
// read; any other system register is unimplemented.
Outcome exec_mrs(Core &core, uint32_t word) {
  constexpr Field kSysReg{.lsb = 5, .width = 15}; // o0:op1:CRn:CRm:op2
  constexpr uint32_t kTpidrroEl0 = 0b1'011'1101'0000'011;
  if (field(word, kSysReg) != kTpidrroEl0) {
    return Outcome::Unimplemented;
  }
  core.set_xr(reg(word, kRt), core.tpidrro_el0(), Width::X);
  return Outcome::Next;
}

Outcome execute(Core &core, Memory &memory, Insn insn, uint32_t word) {
  switch (insn) {
  case Insn::UNKNOWN:
  case Insn::UDF:
    return Outcome::Undefined;
  case Insn::B:
  case Insn::BL:
  case Insn::B_COND:
  case Insn::CBZ:
  case Insn::CBNZ:
  case Insn::TBZ:
  case Insn::TBNZ:
  case Insn::RET:
    return exec_branch(core, insn, word);
  case Insn::SVC:
    return Outcome::Svc;
  case Insn::NOP:
    return Outcome::Next;
  case Insn::CLREX:
    core.clear_exclusive();
    return Outcome::Next;
  case Insn::MRS:
    return exec_mrs(core, word);
  case Insn::CSEL:
  case Insn::CSINC:
    return exec_cond_select(core, insn, word);
  case Insn::LSRV:
    return exec_shift_variable(core, word);
  case Insn::ADR:
  case Insn::ADRP:
    return exec_pc_relative(core, insn, word);
  case Insn::ADD_IMM:
  case Insn::ADDS_IMM:
  case Insn::SUB_IMM:
  case Insn::SUBS_IMM:
    return exec_add_sub_imm(core, insn, word);
  case Insn::MOVZ:
  case Insn::MOVN:
  case Insn::MOVK:
    return exec_move_wide(core, insn, word);
  case Insn::AND_IMM:
  case Insn::ORR_IMM:
    return exec_logical_imm(core, insn, word);
  case Insn::UBFM:
    return exec_ubfm(core, word);
  case Insn::ADD_REG:
  case Insn::SUB_REG:
  case Insn::SUBS_REG:
    return exec_add_sub_reg(core, insn, word);
  case Insn::ORR_REG:
  case Insn::ORN_REG:
  case Insn::EOR_REG:
    return exec_logical_reg(core, insn, word);
  case Insn::LDR_LIT:
  case Insn::LDR_UIMM:
  case Insn::STR_UIMM:
  case Insn::LDRB_UIMM:
  case Insn::LDRH_UIMM:
  case Insn::STRB_PRE:
  case Insn::LDRB_POST:
  case Insn::STP_OFF:
  case Insn::LDP_OFF:
  case Insn::LDR_REG:
  case Insn::LDRB_REG:
  case Insn::STRB_REG:
  case Insn::LDXR:
  case Insn::LDAXR:
  case Insn::STXR:
  case Insn::STLXR:
    return exec_load_store(core, memory, insn, word);
  default:
    return Outcome::Unimplemented;
  }
}

lca_exit exit_at(lca_exit_reason reason, uint64_t pc, uint64_t executed) {
  return {
      .reason = reason,
      .detail = 0,
      .address = pc,
      .executed = executed,
  };
}

lca_exit run(Core &core, Memory &memory, uint64_t limit) {
  uint64_t executed = 0;
  // The entry is a safe point, like the end of every block.
  bool safe_point = true;
  while (executed < limit) {
    const uint64_t pc = core.pc();
    if (safe_point && memory.interrupted()) {
      return exit_at(LCA_EXIT_INTERRUPTED, pc, executed);
    }
    safe_point = false;
    if (pc % kInsnBytes != 0) {
      return exit_at(LCA_EXIT_PC_MISALIGNED, pc, executed);
    }
    const std::optional<uint32_t> word = memory.fetch(pc);
    if (!word) {
      return exit_at(LCA_EXIT_FETCH_ABORT, pc, executed);
    }
    const Insn insn = decode(*word);
    switch (execute(core, memory, insn, *word)) {
    case Outcome::Next:
      core.set_pc(pc + kInsnBytes);
      break;
    case Outcome::Jumped:
      safe_point = true;
      break;
    case Outcome::Svc: {
      core.set_pc(pc + kInsnBytes);
      lca_exit svc = exit_at(LCA_EXIT_SVC, pc, executed + 1);
      svc.detail = field(*word, kImm16);
      return svc;
    }
    case Outcome::Undefined:
      return exit_at(LCA_EXIT_UNDEFINED, pc, executed);
    case Outcome::Unimplemented: {
      lca_exit unimplemented = exit_at(LCA_EXIT_UNIMPLEMENTED, pc, executed);
      unimplemented.detail = static_cast<uint32_t>(insn);
      return unimplemented;
    }
    case Outcome::DataAbort: {
      const Fault fault = memory.fault();
      lca_exit abort = exit_at(LCA_EXIT_DATA_ABORT, fault.address, executed);
      abort.detail = fault.access;
      return abort;
    }
    }
    ++executed;
  }
  return exit_at(LCA_EXIT_LIMIT, core.pc(), executed);
}

} // namespace
} // namespace lince::a64

extern "C" {

lca_status lca_run(lca_state *state, const lca_memory *memory, uint64_t limit,
                   lca_exit *out) noexcept {
  if (state == nullptr || memory == nullptr || out == nullptr ||
      (memory->regions == nullptr && memory->region_count != 0)) {
    return LCA_NULL_ARGUMENT;
  }
  lince::a64::Core core(*state);
  lince::a64::Memory view(*memory);
  *out = lince::a64::run(core, view, limit);
  core.store(*state);
  return LCA_OK;
}

} // extern "C"
