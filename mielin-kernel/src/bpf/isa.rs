#![allow(dead_code)]
//! MielinBPF ISA — instruction set architecture types, encoding, and decoding.

pub use super::BpfError;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Total register count: R0 … R10.
pub const NUM_REGS: usize = 11;
/// R0 holds the return value.
pub const REG_RETURN: u8 = 0;
/// R10 is the read-only frame pointer (top of scratch space).
pub const REG_FRAME: u8 = 10;
/// Maximum number of instructions per program.
pub const MAX_PROGRAM_LEN: usize = 4096;
/// Number of 64-bit scratch words (stack space visible to the program).
pub const SCRATCH_WORDS: usize = 64;
/// Number of scratch bytes.
pub const SCRATCH_BYTES: usize = SCRATCH_WORDS * 8;
/// Default fuel budget for a program run.
pub const DEFAULT_FUEL: u64 = 1 << 16;
/// Minimum fuel budget regardless of program length.
pub const MIN_FUEL: u64 = 16;

// ---------------------------------------------------------------------------
// Opcode byte constants (bits 63–56 of the 64-bit encoding)
// ---------------------------------------------------------------------------

const OPC_ALU: u8 = 0x01;
const OPC_LOADMEM: u8 = 0x02;
const OPC_STOREMEM: u8 = 0x03;
const OPC_LOADCTX: u8 = 0x04;
const OPC_JMP: u8 = 0x05;
const OPC_JMPIF: u8 = 0x06;
const OPC_CALL: u8 = 0x07;
const OPC_EXIT: u8 = 0xFF;

// ---------------------------------------------------------------------------
// Reg
// ---------------------------------------------------------------------------

/// A register index R0 – R10.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reg(pub u8);

impl Reg {
    /// Return the raw index as `usize`.
    #[inline]
    pub const fn idx(self) -> usize {
        self.0 as usize
    }

    /// `true` if the register index is in [0, NUM_REGS).
    #[inline]
    pub const fn is_valid(self) -> bool {
        self.0 < NUM_REGS as u8
    }
}

// ---------------------------------------------------------------------------
// AluOp
// ---------------------------------------------------------------------------

/// Arithmetic / logic operation codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AluOp {
    Add = 0,
    Sub = 1,
    Mul = 2,
    Div = 3,
    Mod = 4,
    And = 5,
    Or = 6,
    Xor = 7,
    Lsh = 8,
    Rsh = 9,
    Arsh = 10,
    Mov = 11,
}

impl AluOp {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Add),
            1 => Some(Self::Sub),
            2 => Some(Self::Mul),
            3 => Some(Self::Div),
            4 => Some(Self::Mod),
            5 => Some(Self::And),
            6 => Some(Self::Or),
            7 => Some(Self::Xor),
            8 => Some(Self::Lsh),
            9 => Some(Self::Rsh),
            10 => Some(Self::Arsh),
            11 => Some(Self::Mov),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Jcc
// ---------------------------------------------------------------------------

/// Jump condition codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Jcc {
    Eq = 0,
    Ne = 1,
    Gt = 2,
    Ge = 3,
    Lt = 4,
    Le = 5,
    Sgt = 6,
    Sge = 7,
    Slt = 8,
    Sle = 9,
}

impl Jcc {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Eq),
            1 => Some(Self::Ne),
            2 => Some(Self::Gt),
            3 => Some(Self::Ge),
            4 => Some(Self::Lt),
            5 => Some(Self::Le),
            6 => Some(Self::Sgt),
            7 => Some(Self::Sge),
            8 => Some(Self::Slt),
            9 => Some(Self::Sle),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Source
// ---------------------------------------------------------------------------

/// Instruction source operand: either a register or an immediate value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Reg(Reg),
    Imm(i64),
}

// ---------------------------------------------------------------------------
// CtxField
// ---------------------------------------------------------------------------

/// Fields accessible via `LoadCtx`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CtxField {
    TracepointId = 0,
    Timestamp = 1,
    CpuId = 2,
    Arg0 = 3,
    Arg1 = 4,
    Arg2 = 5,
}

impl CtxField {
    /// Total number of context fields.
    pub const COUNT: usize = 6;

    /// Convert a raw `u8` to a `CtxField`, or `None` if out of range.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::TracepointId),
            1 => Some(Self::Timestamp),
            2 => Some(Self::CpuId),
            3 => Some(Self::Arg0),
            4 => Some(Self::Arg1),
            5 => Some(Self::Arg2),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// HelperId
// ---------------------------------------------------------------------------

/// Built-in helper function identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HelperId {
    Nop = 0,
    MapLookup = 1,
    MapUpdate = 2,
    MapAdd = 3,
}

impl HelperId {
    /// Convert a raw `u8` to a `HelperId`, or `None` if unknown.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Nop),
            1 => Some(Self::MapLookup),
            2 => Some(Self::MapUpdate),
            3 => Some(Self::MapAdd),
            _ => None,
        }
    }

    /// Number of arguments consumed by each helper.
    pub fn arg_count(self) -> u8 {
        match self {
            Self::Nop => 0,
            Self::MapLookup => 2,
            Self::MapUpdate => 3,
            Self::MapAdd => 3,
        }
    }
}

// ---------------------------------------------------------------------------
// Instruction
// ---------------------------------------------------------------------------

/// A single MielinBPF instruction.
///
/// # Encoding layout (64 bits)
///
/// For most variants:
/// ```text
/// [63:56] opcode  [55:52] dst  [51:48] op/field/src_reg  [47:32] offset/off  [31:0] imm
/// ```
///
/// For `Alu` with register source:
/// ```text
/// [63:56]=0x01  [55:52]=dst  [51:48]=AluOp  [47:44]=src_reg  [43:40]=0  ...
/// ```
/// For `Alu` with immediate source:
/// ```text
/// [63:56]=0x01  [55:52]=dst  [51:48]=AluOp  [47:44]=0  [43:40]=0xF  [31:0]=imm32
/// ```
///
/// For `JmpIf`:
/// ```text
/// [63:56]=0x06  [55:52]=dst  [51:48]=Jcc  [47:32]=off  [31:28]=src_nib  [27:0]=imm28
/// src_nib=0xF → immediate source; else register index
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instruction {
    Alu {
        op: AluOp,
        dst: Reg,
        src: Source,
    },
    LoadMem {
        dst: Reg,
        offset: u16,
    },
    StoreMem {
        src: Source,
        offset: u16,
    },
    LoadCtx {
        dst: Reg,
        field: CtxField,
    },
    Jmp {
        off: i16,
    },
    JmpIf {
        cc: Jcc,
        dst: Reg,
        src: Source,
        off: i16,
    },
    Call {
        helper: HelperId,
    },
    Exit,
}

impl Instruction {
    /// Encode the instruction into a 64-bit word.
    ///
    /// Returns `Err(BpfError::ImmediateTooWide)` if an immediate value does not
    /// fit in an `i32`.
    pub fn to_u64(self) -> Result<u64, BpfError> {
        match self {
            // ----------------------------------------------------------------
            // Exit
            // ----------------------------------------------------------------
            Self::Exit => Ok((OPC_EXIT as u64) << 56),

            // ----------------------------------------------------------------
            // Alu
            // ----------------------------------------------------------------
            Self::Alu { op, dst, src } => {
                let opc = OPC_ALU as u64;
                let dst_bits = (dst.0 as u64 & 0xF) << 52;
                let op_bits = (op as u64 & 0xF) << 48;
                match src {
                    Source::Reg(r) => {
                        // [47:44] = src_reg, [43:40] = 0  → marker 0
                        let src_bits = (r.0 as u64 & 0xF) << 44;
                        Ok((opc << 56) | dst_bits | op_bits | src_bits)
                    }
                    Source::Imm(v) => {
                        let imm32 = imm_to_i32(v)?;
                        // [43:40] = 0xF → immediate marker
                        let marker: u64 = 0xF << 40;
                        let imm_bits = (imm32 as u32) as u64;
                        Ok((opc << 56) | dst_bits | op_bits | marker | imm_bits)
                    }
                }
            }

            // ----------------------------------------------------------------
            // LoadMem
            // ----------------------------------------------------------------
            Self::LoadMem { dst, offset } => {
                let opc = OPC_LOADMEM as u64;
                let dst_bits = (dst.0 as u64 & 0xF) << 52;
                let off_bits = (offset as u64) << 32;
                Ok((opc << 56) | dst_bits | off_bits)
            }

            // ----------------------------------------------------------------
            // StoreMem
            // ----------------------------------------------------------------
            Self::StoreMem { src, offset } => {
                let opc = OPC_STOREMEM as u64;
                let off_bits = (offset as u64) << 32;
                match src {
                    Source::Reg(r) => {
                        let src_bits = (r.0 as u64 & 0xF) << 52;
                        Ok((opc << 56) | src_bits | off_bits)
                    }
                    Source::Imm(v) => {
                        let imm32 = imm_to_i32(v)?;
                        let marker: u64 = 0xF << 52;
                        let imm_bits = (imm32 as u32) as u64;
                        Ok((opc << 56) | marker | off_bits | imm_bits)
                    }
                }
            }

            // ----------------------------------------------------------------
            // LoadCtx
            // ----------------------------------------------------------------
            Self::LoadCtx { dst, field } => {
                let opc = OPC_LOADCTX as u64;
                let dst_bits = (dst.0 as u64 & 0xF) << 52;
                let field_bits = (field as u64 & 0xFF) << 48;
                Ok((opc << 56) | dst_bits | field_bits)
            }

            // ----------------------------------------------------------------
            // Jmp
            // ----------------------------------------------------------------
            Self::Jmp { off } => {
                let opc = OPC_JMP as u64;
                let off_bits = ((off as u16) as u64) << 32;
                Ok((opc << 56) | off_bits)
            }

            // ----------------------------------------------------------------
            // JmpIf
            // ----------------------------------------------------------------
            Self::JmpIf { cc, dst, src, off } => {
                let opc = OPC_JMPIF as u64;
                let dst_bits = (dst.0 as u64 & 0xF) << 52;
                let cc_bits = (cc as u64 & 0xF) << 48;
                let off_bits = ((off as u16) as u64) << 32;
                match src {
                    Source::Reg(r) => {
                        let src_nibble = (r.0 as u64 & 0xF) << 28;
                        Ok((opc << 56) | dst_bits | cc_bits | off_bits | src_nibble)
                    }
                    Source::Imm(v) => {
                        let imm32 = imm_to_i32(v)?;
                        // 0xF in bits [31:28] signals immediate
                        let src_marker: u64 = 0xF << 28;
                        // lower 28 bits of the u32 representation
                        let imm_bits = (imm32 as u32 as u64) & 0x0FFF_FFFF;
                        Ok((opc << 56) | dst_bits | cc_bits | off_bits | src_marker | imm_bits)
                    }
                }
            }

            // ----------------------------------------------------------------
            // Call
            // ----------------------------------------------------------------
            Self::Call { helper } => {
                let opc = OPC_CALL as u64;
                let h_bits = (helper as u64 & 0xFF) << 48;
                Ok((opc << 56) | h_bits)
            }
        }
    }

    /// Decode an instruction from a 64-bit word.
    pub fn from_u64(word: u64) -> Result<Self, BpfError> {
        let opc = (word >> 56) as u8;

        match opc {
            // ----------------------------------------------------------------
            // Exit
            // ----------------------------------------------------------------
            OPC_EXIT => Ok(Self::Exit),

            // ----------------------------------------------------------------
            // Alu
            // ----------------------------------------------------------------
            OPC_ALU => {
                let dst = Reg(((word >> 52) & 0xF) as u8);
                let op_nibble = ((word >> 48) & 0xF) as u8;
                let op = AluOp::from_u8(op_nibble).ok_or(BpfError::InvalidOpcode { word })?;
                // [43:40] == 0xF means immediate
                let marker = ((word >> 40) & 0xF) as u8;
                let src = if marker == 0xF {
                    let imm32 = (word & 0xFFFF_FFFF) as u32 as i32;
                    Source::Imm(imm32 as i64)
                } else {
                    let r = ((word >> 44) & 0xF) as u8;
                    Source::Reg(Reg(r))
                };
                Ok(Self::Alu { op, dst, src })
            }

            // ----------------------------------------------------------------
            // LoadMem
            // ----------------------------------------------------------------
            OPC_LOADMEM => {
                let dst = Reg(((word >> 52) & 0xF) as u8);
                let offset = ((word >> 32) & 0xFFFF) as u16;
                Ok(Self::LoadMem { dst, offset })
            }

            // ----------------------------------------------------------------
            // StoreMem
            // ----------------------------------------------------------------
            OPC_STOREMEM => {
                let offset = ((word >> 32) & 0xFFFF) as u16;
                let marker = ((word >> 52) & 0xF) as u8;
                let src = if marker == 0xF {
                    let imm32 = (word & 0xFFFF_FFFF) as u32 as i32;
                    Source::Imm(imm32 as i64)
                } else {
                    Source::Reg(Reg(marker))
                };
                Ok(Self::StoreMem { src, offset })
            }

            // ----------------------------------------------------------------
            // LoadCtx
            // ----------------------------------------------------------------
            OPC_LOADCTX => {
                let dst = Reg(((word >> 52) & 0xF) as u8);
                let field_byte = ((word >> 48) & 0xFF) as u8;
                let field =
                    CtxField::from_u8(field_byte).ok_or(BpfError::InvalidOpcode { word })?;
                Ok(Self::LoadCtx { dst, field })
            }

            // ----------------------------------------------------------------
            // Jmp
            // ----------------------------------------------------------------
            OPC_JMP => {
                let off = ((word >> 32) & 0xFFFF) as u16 as i16;
                Ok(Self::Jmp { off })
            }

            // ----------------------------------------------------------------
            // JmpIf
            // ----------------------------------------------------------------
            OPC_JMPIF => {
                let dst = Reg(((word >> 52) & 0xF) as u8);
                let cc_nib = ((word >> 48) & 0xF) as u8;
                let cc = Jcc::from_u8(cc_nib).ok_or(BpfError::InvalidOpcode { word })?;
                let off = ((word >> 32) & 0xFFFF) as u16 as i16;
                let src_nib = ((word >> 28) & 0xF) as u8;
                let src = if src_nib == 0xF {
                    // Recover i32 from lower 28 bits (sign-extended from bit 27)
                    let lower28 = (word & 0x0FFF_FFFF) as u32;
                    let imm32 = if lower28 & (1 << 27) != 0 {
                        (lower28 | 0xF000_0000) as i32
                    } else {
                        lower28 as i32
                    };
                    Source::Imm(imm32 as i64)
                } else {
                    Source::Reg(Reg(src_nib))
                };
                Ok(Self::JmpIf { cc, dst, src, off })
            }

            // ----------------------------------------------------------------
            // Call
            // ----------------------------------------------------------------
            OPC_CALL => {
                let h_byte = ((word >> 48) & 0xFF) as u8;
                let helper = HelperId::from_u8(h_byte).ok_or(BpfError::InvalidOpcode { word })?;
                Ok(Self::Call { helper })
            }

            _ => Err(BpfError::InvalidOpcode { word }),
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helper
// ---------------------------------------------------------------------------

/// Return an `i32` from `v`, or `ImmediateTooWide` if it does not fit.
#[inline]
fn imm_to_i32(v: i64) -> Result<i32, BpfError> {
    if v < i32::MIN as i64 || v > i32::MAX as i64 {
        Err(BpfError::ImmediateTooWide { imm: v })
    } else {
        Ok(v as i32)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(insn: Instruction) -> Instruction {
        let word = insn.to_u64().expect("encode");
        Instruction::from_u64(word).expect("decode")
    }

    #[test]
    fn test_reg_idx() {
        assert_eq!(Reg(3).idx(), 3);
        assert!(Reg(10).is_valid());
        assert!(!Reg(11).is_valid());
        assert_eq!(Reg(0).idx(), 0);
    }

    #[test]
    fn test_encode_decode_exit() {
        assert_eq!(roundtrip(Instruction::Exit), Instruction::Exit);
    }

    #[test]
    fn test_encode_decode_alu_reg() {
        let insn = Instruction::Alu {
            op: AluOp::Add,
            dst: Reg(1),
            src: Source::Reg(Reg(2)),
        };
        assert_eq!(roundtrip(insn), insn);
    }

    #[test]
    fn test_encode_decode_alu_imm() {
        let insn = Instruction::Alu {
            op: AluOp::Mov,
            dst: Reg(0),
            src: Source::Imm(42),
        };
        assert_eq!(roundtrip(insn), insn);
    }

    #[test]
    fn test_encode_decode_jmpif() {
        let insn = Instruction::JmpIf {
            cc: Jcc::Eq,
            dst: Reg(1),
            src: Source::Imm(10),
            off: 5,
        };
        assert_eq!(roundtrip(insn), insn);
    }

    #[test]
    fn test_encode_decode_loadmem() {
        let insn = Instruction::LoadMem {
            dst: Reg(2),
            offset: 16,
        };
        assert_eq!(roundtrip(insn), insn);
    }

    #[test]
    fn test_encode_decode_storemem() {
        let insn = Instruction::StoreMem {
            src: Source::Reg(Reg(3)),
            offset: 32,
        };
        assert_eq!(roundtrip(insn), insn);
    }

    #[test]
    fn test_encode_rejects_wide_imm() {
        let insn = Instruction::Alu {
            op: AluOp::Mov,
            dst: Reg(0),
            src: Source::Imm(i32::MAX as i64 + 1),
        };
        assert_eq!(
            insn.to_u64(),
            Err(BpfError::ImmediateTooWide {
                imm: i32::MAX as i64 + 1
            })
        );
    }

    #[test]
    fn test_decode_rejects_bad_opcode() {
        let bad_word: u64 = 0xAB00_0000_0000_0000;
        assert_eq!(
            Instruction::from_u64(bad_word),
            Err(BpfError::InvalidOpcode { word: bad_word })
        );
    }

    #[test]
    fn test_ctxfield_from_u8() {
        assert_eq!(CtxField::from_u8(0), Some(CtxField::TracepointId));
        assert_eq!(CtxField::from_u8(1), Some(CtxField::Timestamp));
        assert_eq!(CtxField::from_u8(2), Some(CtxField::CpuId));
        assert_eq!(CtxField::from_u8(3), Some(CtxField::Arg0));
        assert_eq!(CtxField::from_u8(4), Some(CtxField::Arg1));
        assert_eq!(CtxField::from_u8(5), Some(CtxField::Arg2));
        assert_eq!(CtxField::from_u8(99), None);
    }

    #[test]
    fn test_helperid_from_u8() {
        assert_eq!(HelperId::from_u8(0), Some(HelperId::Nop));
        assert_eq!(HelperId::from_u8(1), Some(HelperId::MapLookup));
        assert_eq!(HelperId::from_u8(2), Some(HelperId::MapUpdate));
        assert_eq!(HelperId::from_u8(3), Some(HelperId::MapAdd));
        assert_eq!(HelperId::from_u8(200), None);
    }

    #[test]
    fn test_encode_decode_loadctx() {
        let insn = Instruction::LoadCtx {
            dst: Reg(0),
            field: CtxField::Arg0,
        };
        assert_eq!(roundtrip(insn), insn);
    }

    #[test]
    fn test_encode_decode_call() {
        let insn = Instruction::Call {
            helper: HelperId::Nop,
        };
        assert_eq!(roundtrip(insn), insn);
    }

    #[test]
    fn test_encode_decode_jmp() {
        let insn = Instruction::Jmp { off: 3 };
        assert_eq!(roundtrip(insn), insn);
    }

    #[test]
    fn test_encode_decode_jmpif_reg() {
        let insn = Instruction::JmpIf {
            cc: Jcc::Ne,
            dst: Reg(2),
            src: Source::Reg(Reg(3)),
            off: 7,
        };
        assert_eq!(roundtrip(insn), insn);
    }

    #[test]
    fn test_encode_decode_alu_all_ops() {
        let ops = [
            AluOp::Add,
            AluOp::Sub,
            AluOp::Mul,
            AluOp::Div,
            AluOp::Mod,
            AluOp::And,
            AluOp::Or,
            AluOp::Xor,
            AluOp::Lsh,
            AluOp::Rsh,
            AluOp::Arsh,
            AluOp::Mov,
        ];
        for op in ops {
            let insn = Instruction::Alu {
                op,
                dst: Reg(0),
                src: Source::Imm(1),
            };
            assert_eq!(roundtrip(insn), insn, "roundtrip failed for {:?}", op);
        }
    }

    #[test]
    fn test_alu_neg_imm() {
        let insn = Instruction::Alu {
            op: AluOp::Mov,
            dst: Reg(0),
            src: Source::Imm(-1),
        };
        assert_eq!(roundtrip(insn), insn);
    }
}
