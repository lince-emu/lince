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
constexpr Field kRm{.lsb = 16, .width = 5};
constexpr Field kRt2{.lsb = 10, .width = 5};
constexpr Field kImm6{.lsb = 10, .width = 6};
constexpr Field kImm7{.lsb = 15, .width = 7};
constexpr Field kImm9{.lsb = 12, .width = 9};
constexpr Field kShift{.lsb = 22, .width = 2};
constexpr Field kInvert{.lsb = 21, .width = 1}; // N of the logical (shifted register) forms
constexpr Field kBitN{.lsb = 22, .width = 1};   // N of the bitmask immediate forms
constexpr Field kImmr{.lsb = 16, .width = 6};
constexpr Field kImms{.lsb = 10, .width = 6};
constexpr Field kSize{.lsb = 30, .width = 2}; // log2 of the access size in bytes
constexpr Field kLoad{.lsb = 22, .width = 1}; // L of the unsigned-offset forms
constexpr Field kPreIndex{.lsb = 11, .width = 1};
constexpr Field kLitOpc{.lsb = 30, .width = 1};  // LDR (literal): 0 is W, 1 is X
constexpr Field kPairOpc{.lsb = 31, .width = 1}; // LDP/STP: 0 is W, 1 is X
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

// A data access that failed on both paths (LCA_EXIT_DATA_ABORT).
struct Fault {
  uint64_t address;
  uint32_t access; // LCA_PERM_READ or LCA_PERM_WRITE
};

// Guest memory as one lca_run call sees it.
class Memory {
public:
  explicit Memory(const lca_memory &view)
      : host_(view.host_base, view.host_size), regions_(view.regions, view.region_count),
        callbacks_(view.callbacks) {}

  // Inline accesses are plain copies: no single-copy atomicity or ordering, which only a
  // single core running per lca_run call allows. Revisit when several cores share memory.
  //
  // Copies the guest bytes at `addr` into `out`: inline when one readable region holds them,
  // else through the slow path. False (and the fault recorded) when neither can.
  [[nodiscard]] bool read(uint64_t addr, std::span<uint8_t> out) {
    if (const auto bytes = inline_bytes(addr, out.size(), LCA_PERM_READ)) {
      std::memcpy(out.data(), bytes->data(), out.size());
      return true;
    }
    if (callbacks_.read != nullptr &&
        callbacks_.read(callbacks_.context, addr, out.data(), out.size()) == 0) {
      return true;
    }
    fault_ = {.address = addr, .access = LCA_PERM_READ};
    return false;
  }

  // Copies `data` to the guest bytes at `addr`, like `read`.
  [[nodiscard]] bool write(uint64_t addr, std::span<const uint8_t> data) {
    if (const auto bytes = inline_bytes(addr, data.size(), LCA_PERM_WRITE)) {
      std::memcpy(bytes->data(), data.data(), data.size());
      return true;
    }
    if (callbacks_.write != nullptr &&
        callbacks_.write(callbacks_.context, addr, data.data(), data.size()) == 0) {
      return true;
    }
    fault_ = {.address = addr, .access = LCA_PERM_WRITE};
    return false;
  }

  // The last failed access.
  [[nodiscard]] Fault fault() const { return fault_; }

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
  lca_callbacks callbacks_;
  Fault fault_{};
};

// What one instruction did.
enum class Outcome : uint8_t {
  Next,          // continue with pc + 4
  Jumped,        // pc was set by the instruction
  Svc,           // continue with pc + 4, then exit to Rust
  Undefined,     // reserved encoding: exit before executing it
  Unimplemented, // decoded but without semantics: exit before executing it
  DataAbort,     // a data access failed: exit with the instruction not executed
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

// One data access: `size` bytes at guest address `addr`.
struct Access {
  uint64_t addr;
  std::size_t size;
};

// The zero-extended little-endian value of the access, or nullopt on a fault.
std::optional<uint64_t> load(Memory &memory, Access access) {
  std::array<uint8_t, sizeof(uint64_t)> bytes{};
  if (!memory.read(access.addr, std::span(bytes).first(access.size))) {
    return std::nullopt;
  }
  uint64_t value = 0;
  std::memcpy(&value, bytes.data(), sizeof value);
  return value;
}

// Stores the low `access.size` bytes of `value`; false on a fault.
bool store(Memory &memory, Access access, uint64_t value) {
  std::array<uint8_t, sizeof(uint64_t)> bytes{};
  std::memcpy(bytes.data(), &value, sizeof value);
  return memory.write(access.addr, std::span<const uint8_t>(bytes).first(access.size));
}

// The access of a load/store form at `addr`; its size field is log2 of the size in bytes.
Access sized_access(uint32_t word, uint64_t addr) {
  return {.addr = addr, .size = std::size_t{1} << field(word, kSize)};
}

// Loads the access into Rt (zero-extended) or stores the low bytes of Rt.
Outcome transfer(Core &core, Memory &memory, Reg rt, Access access, bool is_load) {
  if (!is_load) {
    return store(memory, access, core.xr(rt)) ? Outcome::Next : Outcome::DataAbort;
  }
  const std::optional<uint64_t> value = load(memory, access);
  if (!value) {
    return Outcome::DataAbort;
  }
  core.set_xr(rt, *value, Width::X);
  return Outcome::Next;
}

// LDR (literal): a W or X load from pc + imm19 * 4.
Outcome exec_load_literal(Core &core, Memory &memory, uint32_t word) {
  const uint64_t offset =
      sign_extend<kImm19.width + kOffsetShift>(uint64_t{field(word, kImm19)} << kOffsetShift);
  const std::size_t size = field(word, kLitOpc) != 0 ? sizeof(uint64_t) : sizeof(uint32_t);
  const std::optional<uint64_t> value = load(memory, {.addr = core.pc() + offset, .size = size});
  if (!value) {
    return Outcome::DataAbort;
  }
  core.set_xr(reg(word, kRt), *value, Width::X);
  return Outcome::Next;
}

// LDR/STR/LDRB/LDRH (unsigned offset): base plus imm12 scaled by the access size.
Outcome exec_load_store_uimm(Core &core, Memory &memory, uint32_t word) {
  const uint64_t offset = uint64_t{field(word, kImm12)} << field(word, kSize);
  const uint64_t addr = core.xs(reg(word, kRn)) + offset;
  return transfer(core, memory, reg(word, kRt), sized_access(word, addr), field(word, kLoad) != 0);
}

// STRB (pre-index) and LDRB (post-index): base plus signed imm9, written back after the access.
Outcome exec_load_store_indexed(Core &core, Memory &memory, Insn insn, uint32_t word) {
  const Reg rn = reg(word, kRn);
  if (rn == reg(word, kRt) && rn != kZr) {
    // CONSTRAINED UNPREDICTABLE writeback with Rn == Rt: UNDEFINED is one permitted choice.
    // UNKNOWN: which choice the Switch 2 cores make (docs/08-switch2-hardware.md §4).
    return Outcome::Undefined;
  }
  const uint64_t base = core.xs(rn);
  const uint64_t updated = base + sign_extend<kImm9.width>(field(word, kImm9));
  const uint64_t addr = field(word, kPreIndex) != 0 ? updated : base;
  const Outcome outcome =
      transfer(core, memory, reg(word, kRt), sized_access(word, addr), insn == Insn::LDRB_POST);
  if (outcome == Outcome::Next) {
    core.set_xs(rn, updated, Width::X);
  }
  return outcome;
}

// LDP/STP (signed offset): two W or X registers at base plus imm7 scaled, as one access, so a
// fault changes nothing.
Outcome exec_pair(Core &core, Memory &memory, Insn insn, uint32_t word) {
  const bool x = field(word, kPairOpc) != 0;
  const std::size_t size = x ? sizeof(uint64_t) : sizeof(uint32_t);
  const unsigned scale = x ? 3 : 2;
  const uint64_t addr =
      core.xs(reg(word, kRn)) + (sign_extend<kImm7.width>(field(word, kImm7)) << scale);
  const Reg rt = reg(word, kRt);
  const Reg rt2 = reg(word, kRt2);
  std::array<uint8_t, 2 * sizeof(uint64_t)> bytes{};
  const std::span<uint8_t> pair = std::span(bytes).first(2 * size);
  if (insn == Insn::STP_OFF) {
    const uint64_t first = core.xr(rt);
    const uint64_t second = core.xr(rt2);
    std::memcpy(pair.data(), &first, size);
    std::memcpy(pair.subspan(size).data(), &second, size);
    return memory.write(addr, pair) ? Outcome::Next : Outcome::DataAbort;
  }
  if (rt == rt2) {
    // CONSTRAINED UNPREDICTABLE load pair with Rt == Rt2: UNDEFINED is one permitted choice.
    // UNKNOWN: which choice the Switch 2 cores make (docs/08-switch2-hardware.md §4).
    return Outcome::Undefined;
  }
  if (!memory.read(addr, pair)) {
    return Outcome::DataAbort;
  }
  uint64_t first = 0;
  uint64_t second = 0;
  std::memcpy(&first, pair.data(), size);
  std::memcpy(&second, pair.subspan(size).data(), size);
  core.set_xr(rt, first, Width::X);
  core.set_xr(rt2, second, Width::X);
  return Outcome::Next;
}

Outcome execute(Core &core, Memory &memory, Insn insn, uint32_t word) {
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
    return exec_load_literal(core, memory, word);
  case Insn::LDR_UIMM:
  case Insn::STR_UIMM:
  case Insn::LDRB_UIMM:
  case Insn::LDRH_UIMM:
    return exec_load_store_uimm(core, memory, word);
  case Insn::STRB_PRE:
  case Insn::LDRB_POST:
    return exec_load_store_indexed(core, memory, insn, word);
  case Insn::STP_OFF:
  case Insn::LDP_OFF:
    return exec_pair(core, memory, insn, word);
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
    switch (execute(core, memory, insn, *word)) {
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
