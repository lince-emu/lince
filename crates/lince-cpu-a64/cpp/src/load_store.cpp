// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Tier A semantics of the load and store forms (ADR-0005), from the pseudocode of the Arm
// Architecture Reference Manual for A-profile (public, LEGAL-11).

#include "machine.hpp"

#include <array>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <optional>
#include <span>

namespace lince::a64 {
namespace {

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

} // namespace

Outcome exec_load_store(Core &core, Memory &memory, Insn insn, uint32_t word) {
  switch (insn) {
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

} // namespace lince::a64
