// SPDX-FileCopyrightText: 2026 Lince contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// The guest core and guest memory as the Tier A interpreter sees them during one lca_run call,
// shared by the instruction groups (interpreter.cpp, load_store.cpp). Built without exceptions
// or RTTI; no threads, I/O or mutable globals (ARCH-51, ARCH-52).

#ifndef LINCE_CPU_A64_MACHINE_HPP
#define LINCE_CPU_A64_MACHINE_HPP

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
constexpr Field kOption{.lsb = 13, .width = 3};  // extend of the register-offset forms
constexpr Field kScaled{.lsb = 12, .width = 1};  // S of the register-offset forms
constexpr Field kRs = kRm;                       // status register of the store-exclusives
constexpr Field kAcqRel{.lsb = 15, .width = 1};  // o0 of the exclusives: acquire or release
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

  // Atomically replaces the `size` bytes at the aligned `addr` with `desired` if they hold
  // `expected`, through the compare_exchange callback (lca_callbacks). Whether it stored, or
  // nullopt (and the fault recorded) when the bytes are not mapped writable.
  [[nodiscard]] std::optional<bool> compare_exchange(uint64_t addr, uint64_t size,
                                                     uint64_t expected, uint64_t desired) {
    uint32_t stored = 0;
    if (callbacks_.compare_exchange != nullptr &&
        callbacks_.compare_exchange(callbacks_.context, addr, size, expected, desired, &stored) ==
            0) {
      return stored != 0;
    }
    fault_ = {.address = addr, .access = LCA_PERM_WRITE};
    return std::nullopt;
  }

  // Records an alignment fault of an exclusive access at `addr`.
  void misaligned(uint64_t addr, uint32_t access) { fault_ = {.address = addr, .access = access}; }

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
  explicit Core(const lca_state &state)
      : sp_(state.sp), pc_(state.pc), nzcv_(state.nzcv), tpidrro_el0_(state.tpidrro_el0),
        exclusive_(state.exclusive) {
    std::ranges::copy(state.x, x_.begin());
  }

  void store(lca_state &state) const {
    std::copy_n(x_.begin(), std::size(state.x), std::begin(state.x));
    state.sp = sp_;
    state.pc = pc_;
    state.nzcv = nzcv_;
    state.tpidrro_el0 = tpidrro_el0_;
    state.exclusive = exclusive_;
  }

  [[nodiscard]] uint64_t tpidrro_el0() const { return tpidrro_el0_; }

  // The exclusive monitor: the reservation of the last load-exclusive, if any.
  [[nodiscard]] const lca_exclusive &exclusive() const { return exclusive_; }
  void reserve(uint64_t address, uint64_t value, uint32_t size) {
    exclusive_ = {.address = address, .value = value, .size = size, .reserved = 0};
  }
  void clear_exclusive() { exclusive_ = {}; }

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
  uint64_t tpidrro_el0_;
  lca_exclusive exclusive_;
};

// Executes a load or store form (load_store.cpp).
Outcome exec_load_store(Core &core, Memory &memory, Insn insn, uint32_t word);

} // namespace lince::a64

#endif // LINCE_CPU_A64_MACHINE_HPP
