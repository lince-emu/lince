// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Tier A interpreter (ADR-0005): the reference semantics of every decoded form, executed one
// instruction at a time. Semantics follow the pseudocode of the Arm Architecture Reference
// Manual for A-profile (public, LEGAL-11). Built without exceptions or RTTI; no threads, I/O or
// mutable globals (ARCH-51, ARCH-52).

#include "decode.hpp"

#include "lince-cpu-a64.h"

#include <algorithm>
#include <array>
#include <bit>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <optional>
#include <span>

namespace lince::a64 {
namespace {

static_assert(std::endian::native == std::endian::little,
              "guest memory is little-endian and is copied as host integers");

constexpr unsigned kXBits = 64;
constexpr unsigned kWBits = 32;
constexpr uint64_t kWMask = 0xffff'ffff;
constexpr uint32_t kInsnBytes = 4;
constexpr std::size_t kRegisters = 31; // X0 to X30; number 31 is SP or the zero register
constexpr unsigned kOffsetShift = 2;   // branch offsets count instructions
constexpr unsigned kPageBits = 12;
constexpr uint32_t kFlagN = 1U << 31U;
constexpr uint32_t kFlagZ = 1U << 30U;
constexpr uint32_t kFlagC = 1U << 29U;
constexpr uint32_t kFlagV = 1U << 28U;

// An instruction field: `width` bits starting at bit `lsb` (Arm ARM encoding diagrams).
struct Field {
  unsigned lsb;
  unsigned width;
};

constexpr Field kRd{.lsb = 0, .width = 5};
constexpr Field kRt = kRd;
constexpr Field kRn{.lsb = 5, .width = 5};
constexpr Field kSf{.lsb = 31, .width = 1};
constexpr Field kImm26{.lsb = 0, .width = 26};
constexpr Field kImm19{.lsb = 5, .width = 19};
constexpr Field kImm16{.lsb = 5, .width = 16};
constexpr Field kImm14{.lsb = 5, .width = 14};
constexpr Field kImm12{.lsb = 10, .width = 12};
constexpr Field kCond{.lsb = 0, .width = 4};
constexpr Field kB5{.lsb = 31, .width = 1};
constexpr Field kB40{.lsb = 19, .width = 5};
constexpr Field kImmLo{.lsb = 29, .width = 2};
constexpr Field kSh{.lsb = 22, .width = 1};
constexpr Field kHw{.lsb = 21, .width = 2};
constexpr unsigned kImm12Shift = 12;
constexpr unsigned kHwScale = 16;
constexpr unsigned kB5Shift = 5;

constexpr uint32_t field(uint32_t word, Field f) {
  return (word >> f.lsb) & ((1U << f.width) - 1U);
}

// The low `Bits` bits of `value` as a two's complement number, extended to 64 bits.
template <unsigned Bits> constexpr uint64_t sign_extend(uint64_t value) {
  static_assert(Bits > 0 && Bits < kXBits);
  constexpr uint64_t kSign = uint64_t{1} << (Bits - 1);
  const uint64_t low = value & ((uint64_t{1} << Bits) - 1);
  return (low ^ kSign) - kSign;
}

// A register number from an instruction field.
enum class Reg : uint8_t {};
constexpr Reg kZr{31}; // register 31: the zero register or SP, by instruction
constexpr Reg kLinkRegister{30};

constexpr Reg reg(uint32_t word, Field f) { return static_cast<Reg>(field(word, f)); }

// The operand width an instruction's sf bit selects.
enum class Width : uint8_t { W, X };

constexpr Width width(uint32_t word) { return field(word, kSf) != 0 ? Width::X : Width::W; }

// Guest memory as one lca_run call sees it.
class Memory {
public:
  explicit Memory(const lca_memory &view)
      : host_(view.host_base, view.host_size), regions_(view.regions, view.region_count) {}

  // The 32-bit word at `addr` if `addr` is in an executable region.
  [[nodiscard]] std::optional<uint32_t> fetch(uint64_t addr) const {
    const std::optional<std::span<uint8_t>> bytes =
        inline_bytes(addr, kInsnBytes, LCA_PERM_EXECUTE);
    if (!bytes) {
      return std::nullopt;
    }
    uint32_t word = 0;
    std::memcpy(&word, bytes->data(), sizeof word);
    return word;
  }

private:
  // The host bytes of [addr, addr + size) when one region holds them with `perm`.
  [[nodiscard]] std::optional<std::span<uint8_t>> inline_bytes(uint64_t addr, uint64_t size,
                                                               uint32_t perm) const {
    for (const lca_region &region : regions_) {
      const bool inside = addr >= region.base && addr - region.base <= region.size &&
                          size <= region.size - (addr - region.base);
      if (!inside) {
        continue;
      }
      if ((region.perm & perm) != perm || addr > host_.size() || size > host_.size() - addr) {
        return std::nullopt;
      }
      return host_.subspan(addr, size);
    }
    return std::nullopt;
  }

  std::span<uint8_t> host_;
  std::span<const lca_region> regions_;
};

// What one instruction did.
enum class Outcome : uint8_t {
  Next,          // continue with pc + 4
  Jumped,        // pc was set by the instruction
  Svc,           // continue with pc + 4, then exit to Rust
  Undefined,     // reserved encoding: exit before executing it
  Unimplemented, // decoded but without semantics: exit before executing it
};

// The core's registers while lca_run executes.
class Core {
public:
  explicit Core(const lca_state &state) : sp_(state.sp), pc_(state.pc), nzcv_(state.nzcv) {
    std::ranges::copy(state.x, x_.begin());
  }

  void store(lca_state &state) const {
    std::copy_n(x_.begin(), std::size(state.x), std::begin(state.x));
    state.sp = sp_;
    state.pc = pc_;
    state.nzcv = nzcv_;
  }

  [[nodiscard]] uint64_t pc() const { return pc_; }
  void set_pc(uint64_t pc) { pc_ = pc; }
  [[nodiscard]] uint32_t nzcv() const { return nzcv_; }
  void set_nzcv(uint32_t nzcv) { nzcv_ = nzcv; }

  // Register `r` where 31 reads as zero.
  [[nodiscard]] uint64_t xr(Reg r) const { return r == kZr ? 0 : x_.at(index(r)); }
  // Register `r` where 31 is SP.
  [[nodiscard]] uint64_t xs(Reg r) const { return r == kZr ? sp_ : x_.at(index(r)); }

  // Writes register `r` (31 discards); a 32-bit result clears the upper half.
  void set_xr(Reg r, uint64_t value, Width w) {
    if (r != kZr) {
      x_.at(index(r)) = w == Width::X ? value : value & kWMask;
    }
  }
  // Writes register `r` (31 is SP); a 32-bit result clears the upper half.
  void set_xs(Reg r, uint64_t value, Width w) {
    const uint64_t v = w == Width::X ? value : value & kWMask;
    if (r == kZr) {
      sp_ = v;
    } else {
      x_.at(index(r)) = v;
    }
  }

private:
  static std::size_t index(Reg r) { return static_cast<std::size_t>(r); }

  std::array<uint64_t, kRegisters> x_{};
  uint64_t sp_;
  uint64_t pc_;
  uint32_t nzcv_;
};

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
  const uint64_t value = insn == Insn::MOVK ? (core.xr(rd) & ~(kHalfword << shift)) | imm : imm;
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

Outcome execute(Core &core, Insn insn, uint32_t word) {
  switch (insn) {
  case Insn::UNKNOWN:
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
  case Insn::ADR:
  case Insn::ADRP:
    return exec_pc_relative(core, insn, word);
  case Insn::ADD_IMM:
  case Insn::ADDS_IMM:
  case Insn::SUB_IMM:
  case Insn::SUBS_IMM:
    return exec_add_sub_imm(core, insn, word);
  case Insn::MOVZ:
  case Insn::MOVK:
    return exec_move_wide(core, insn, word);
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

lca_exit run(Core &core, const Memory &memory, uint64_t limit) {
  uint64_t executed = 0;
  while (executed < limit) {
    const uint64_t pc = core.pc();
    if (pc % kInsnBytes != 0) {
      return exit_at(LCA_EXIT_PC_MISALIGNED, pc, executed);
    }
    const std::optional<uint32_t> word = memory.fetch(pc);
    if (!word) {
      return exit_at(LCA_EXIT_FETCH_ABORT, pc, executed);
    }
    const Insn insn = decode(*word);
    switch (execute(core, insn, *word)) {
    case Outcome::Next:
      core.set_pc(pc + kInsnBytes);
      break;
    case Outcome::Jumped:
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
  const lince::a64::Memory view(*memory);
  *out = lince::a64::run(core, view, limit);
  core.store(*state);
  return LCA_OK;
}

} // extern "C"
