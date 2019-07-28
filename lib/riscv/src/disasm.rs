pub const REG_NAMES : [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2",
    "s0", "s1", "a0", "a1", "a2", "a3", "a4", "a5",
    "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7",
    "s8", "s9", "s10", "s11", "t3", "t4", "t5", "t6"
];

pub fn register_name(reg: u8) -> &'static str {
    REG_NAMES[reg as usize]
}

use super::op::{Op};

pub fn mnemonic(op: &Op) -> &'static str {
    match *op {
        Op::Illegal {..} => "illegal",
        Op::Lb {..} => "lb",
        Op::Lh {..} => "lh",
        Op::Lw {..} => "lw",
        Op::Ld {..} => "ld",
        Op::Lbu {..} => "lbu",
        Op::Lhu {..} => "lhu",
        Op::Lwu {..} => "lwu",
        Op::Fence {..} => "fence",
        Op::FenceI {..} => "fence.i",
        Op::Addi {..} => "addi",
        Op::Slli {..} => "slli",
        Op::Slti {..} => "slti",
        Op::Sltiu {..} => "sltiu",
        Op::Xori {..} => "xori",
        Op::Srli {..} => "srli",
        Op::Srai {..} => "srai",
        Op::Ori {..} => "ori",
        Op::Andi {..} => "andi",
        Op::Auipc {..} => "auipc",
        Op::Addiw {..} => "addiw",
        Op::Slliw {..} => "slliw",
        Op::Srliw {..} => "srliw",
        Op::Sraiw {..} => "sraiw",
        Op::Sb {..} => "sb",
        Op::Sh {..} => "sh",
        Op::Sw {..} => "sw",
        Op::Sd {..} => "sd",
        Op::Add {..} => "add",
        Op::Sub {..} => "sub",
        Op::Sll {..} => "sll",
        Op::Slt {..} => "slt",
        Op::Sltu {..} => "sltu",
        Op::Xor {..} => "xor",
        Op::Srl {..} => "srl",
        Op::Sra {..} => "sra",
        Op::Or {..} => "or",
        Op::And {..} => "and",
        Op::Lui {..} => "lui",
        Op::Addw {..} => "addw",
        Op::Subw {..} => "subw",
        Op::Sllw {..} => "sllw",
        Op::Srlw {..} => "srlw",
        Op::Sraw {..} => "sraw",
        Op::Beq {..} => "beq",
        Op::Bne {..} => "bne",
        Op::Blt {..} => "blt",
        Op::Bge {..} => "bge",
        Op::Bltu {..} => "bltu",
        Op::Bgeu {..} => "bgeu",
        Op::Jalr {..} => "jalr",
        Op::Jal {..} => "jal",
        Op::Ecall {..} => "ecall",
        Op::Ebreak {..} => "ebreak",
        Op::Csrrw {..} => "csrrw",
        Op::Csrrs {..} => "csrrs",
        Op::Csrrc {..} => "csrrc",
        Op::Csrrwi {..} => "csrrwi",
        Op::Csrrsi {..} => "csrrsi",
        Op::Csrrci {..} => "csrrci",
        Op::Mul {..} => "mul",
        Op::Mulh {..} => "mulh",
        Op::Mulhsu {..} => "mulhsu",
        Op::Mulhu {..} => "mulhu",
        Op::Div {..} => "div",
        Op::Divu {..} => "divu",
        Op::Rem {..} => "rem",
        Op::Remu {..} => "remu",
        Op::Mulw {..} => "mulw",
        Op::Divw {..} => "divw",
        Op::Divuw {..} => "divuw",
        Op::Remw {..} => "remw",
        Op::Remuw {..} => "remuw",
        Op::LrW {..} => "lr.w",
        Op::LrD {..} => "lr.d",
        Op::ScW {..} => "sc.w",
        Op::ScD {..} => "sc.d",
        Op::AmoswapW {..} => "amoswap.w",
        Op::AmoswapD {..} => "amoswap.d",
        Op::AmoaddW {..} => "amoadd.w",
        Op::AmoaddD {..} => "amoadd.d",
        Op::AmoxorW {..} => "amoxor.w",
        Op::AmoxorD {..} => "amoxor.d",
        Op::AmoandW {..} => "amoand.w",
        Op::AmoandD {..} => "amoand.d",
        Op::AmoorW {..} => "amoor.w",
        Op::AmoorD {..} => "amoor.d",
        Op::AmominW {..} => "amomin.w",
        Op::AmominD {..} => "amomin.d",
        Op::AmomaxW {..} => "amomax.w",
        Op::AmomaxD {..} => "amomax.d",
        Op::AmominuW {..} => "amominu.w",
        Op::AmominuD {..} => "amominu.d",
        Op::AmomaxuW {..} => "amomaxu.w",
        Op::AmomaxuD {..} => "amomaxu.d",
        Op::Flw {..} => "flw",
        Op::Fsw {..} => "fsw",
        Op::FaddS {..} => "fadd.s",
        Op::FsubS {..} => "fsub.s",
        Op::FmulS {..} => "fmul.s",
        Op::FdivS {..} => "fdiv.s",
        Op::FsqrtS {..} => "fsqrt.s",
        Op::FsgnjS {..} => "fsgnj.s",
        Op::FsgnjnS {..} => "fsgnjn.s",
        Op::FsgnjxS {..} => "fsgnjx.s",
        Op::FminS {..} => "fmin.s",
        Op::FmaxS {..} => "fmax.s",
        Op::FcvtWS {..} => "fcvt.w.s",
        Op::FcvtWuS {..} => "fcvt.wu.s",
        Op::FcvtLS {..} => "fcvt.l.s",
        Op::FcvtLuS {..} => "fcvt.lu.s",
        Op::FmvXW {..} => "fmv.x.w",
        Op::FclassS {..} => "fclass.s",
        Op::FeqS {..} => "feq.s",
        Op::FltS {..} => "flt.s",
        Op::FleS {..} => "fle.s",
        Op::FcvtSW {..} => "fcvt.s.w",
        Op::FcvtSWu {..} => "fcvt.s.wu",
        Op::FcvtSL {..} => "fcvt.s.l",
        Op::FcvtSLu {..} => "fcvt.s.lu",
        Op::FmvWX {..} => "fmv.w.x",
        Op::FmaddS {..} => "fmadd.s",
        Op::FmsubS {..} => "fmsub.s",
        Op::FnmsubS {..} => "fnmsub.s",
        Op::FnmaddS {..} => "fnmadd.s",
        Op::Fld {..} => "fld",
        Op::Fsd {..} => "fsd",
        Op::FaddD {..} => "fadd.d",
        Op::FsubD {..} => "fsub.d",
        Op::FmulD {..} => "fmul.d",
        Op::FdivD {..} => "fdiv.d",
        Op::FsqrtD {..} => "fsqrt.d",
        Op::FsgnjD {..} => "fsgnj.d",
        Op::FsgnjnD {..} => "fsgnjn.d",
        Op::FsgnjxD {..} => "fsgnjx.d",
        Op::FminD {..} => "fmin.d",
        Op::FmaxD {..} => "fmax.d",
        Op::FcvtSD {..} => "fcvt.s.d",
        Op::FcvtDS {..} => "fcvt.d.s",
        Op::FcvtWD {..} => "fcvt.w.d",
        Op::FcvtWuD {..} => "fcvt.wu.d",
        Op::FcvtLD {..} => "fcvt.l.d",
        Op::FcvtLuD {..} => "fcvt.lu.d",
        Op::FmvXD {..} => "fmv.x.d",
        Op::FclassD {..} => "fclass.d",
        Op::FeqD {..} => "feq.d",
        Op::FltD {..} => "flt.d",
        Op::FleD {..} => "fle.d",
        Op::FcvtDW {..} => "fcvt.d.w",
        Op::FcvtDWu {..} => "fcvt.d.wu",
        Op::FcvtDL {..} => "fcvt.d.l",
        Op::FcvtDLu {..} => "fcvt.d.lu",
        Op::FmvDX {..} => "fmv.d.x",
        Op::FmaddD {..} => "fmadd.d",
        Op::FmsubD {..} => "fmsub.d",
        Op::FnmsubD {..} => "fnmsub.d",
        Op::FnmaddD {..} => "fnmadd.d",
        Op::Sret {..} => "sret",
        Op::Wfi {..} => "wfi",
        Op::SfenceVma {..} => "sfence.vma",
    }
}

#[cfg(feature = "std")]
pub fn print_instr(pc: u64, bits: u32, inst: &Op) {
    let mnemonic = mnemonic(inst);

    if (pc & 0xFFFFFFFF) == pc {
        eprint!("{:8x}:       ", pc);
    } else {
        eprint!("{:16x}:       ", pc);
    }

    if bits & 3 == 3 {
        eprint!("{:08x}", bits);
    } else {
        eprint!("{:04x}    ", bits & 0xFFFF);
    }

    eprint!("        {:-7} ", mnemonic);

    match *inst {
        Op::Illegal => (),
        Op::Lui { rd, imm } |
        Op::Auipc { rd, imm } =>
            eprint!("{}, {:#x}",  register_name(rd), (imm as u32) >> 12),
        Op::Jal { rd, imm } => {
            // Offset the immediate. Check out decode.rs for more details.
            let imm = imm.wrapping_sub(if bits & 3 == 3 { 0 } else { 2 });
            let target_pc = pc.wrapping_add(imm as u64);
            let (sign, imm) = if imm < 0 {
                ('-', -imm)
            } else {
                ('+', imm)
            };
            eprint!("{}, pc {} {} <{:x}>",  register_name(rd), sign, imm, target_pc)
        }
        Op::Beq { rs1, rs2, imm } |
        Op::Bne { rs1, rs2, imm } |
        Op::Blt { rs1, rs2, imm } |
        Op::Bge { rs1, rs2, imm } |
        Op::Bltu { rs1, rs2, imm } |
        Op::Bgeu { rs1, rs2, imm } => {
            // Offset the immediate. Check out decode.rs for more details.
            let imm = imm.wrapping_sub(if bits & 3 == 3 { 0 } else { 2 });
            let target_pc = pc.wrapping_add(imm as u64);
            let (sign, imm) = if imm < 0 {
                ('-', -imm)
            } else {
                ('+', imm)
            };
            eprint!("{}, {}, pc {} {} <{:x}>",  register_name(rs1), register_name(rs2), sign, imm, target_pc)
        }
        Op::Lb { rd, rs1, imm } |
        Op::Lh { rd, rs1, imm } |
        Op::Lw { rd, rs1, imm } |
        Op::Ld { rd, rs1, imm } |
        Op::Lbu { rd, rs1, imm } |
        Op::Lhu { rd, rs1, imm } |
        Op::Lwu { rd, rs1, imm } |
        // jalr has same string representation as load instructions.
        Op::Jalr { rd, rs1, imm } =>
            eprint!("{}, {}({})", register_name(rd), imm, register_name(rs1)),
        // TODO: display the arguments of fence/sfence.vma?
        Op::Fence |
        Op::FenceI |
        Op::Ecall |
        Op::Ebreak |
        Op::Sret |
        Op::Wfi |
        Op::SfenceVma {..} => (),
        Op::Sb { rs1, rs2, imm } |
        Op::Sh { rs1, rs2, imm } |
        Op::Sw { rs1, rs2, imm } |
        Op::Sd { rs1, rs2, imm } =>
            eprint!("{}, {}({})", register_name(rs2), imm, register_name(rs1)),
        Op::Addi { rd, rs1, imm } |
        Op::Slti { rd, rs1, imm } |
        Op::Sltiu { rd, rs1, imm } |
        Op::Xori { rd, rs1, imm } |
        Op::Ori { rd, rs1, imm } |
        Op::Andi { rd, rs1, imm } |
        Op::Addiw { rd, rs1, imm } |
        // The shifts technically should have a unsigned argument, but since immediates for shifts are small numbers,
        // converting to sreg_t does not hurt.
        Op::Slli { rd, rs1, imm } |
        Op::Srli { rd, rs1, imm } |
        Op::Srai { rd, rs1, imm } |
        Op::Slliw { rd, rs1, imm } |
        Op::Srliw { rd, rs1, imm } |
        Op::Sraiw { rd, rs1, imm } =>
            eprint!("{}, {}, {}", register_name(rd), register_name(rs1), imm),
        Op::Add { rd, rs1, rs2 } |
        Op::Sub { rd, rs1, rs2 } |
        Op::Sll { rd, rs1, rs2 } |
        Op::Slt { rd, rs1, rs2 } |
        Op::Sltu { rd, rs1, rs2 } |
        Op::Xor { rd, rs1, rs2 } |
        Op::Srl { rd, rs1, rs2 } |
        Op::Sra { rd, rs1, rs2 } |
        Op::Or { rd, rs1, rs2 } |
        Op::And { rd, rs1, rs2 } |
        Op::Addw { rd, rs1, rs2 } |
        Op::Subw { rd, rs1, rs2 } |
        Op::Sllw { rd, rs1, rs2 } |
        Op::Srlw { rd, rs1, rs2 } |
        Op::Sraw { rd, rs1, rs2 } |
        Op::Mul { rd, rs1, rs2 } |
        Op::Mulh { rd, rs1, rs2 } |
        Op::Mulhsu { rd, rs1, rs2 } |
        Op::Mulhu { rd, rs1, rs2 } |
        Op::Div { rd, rs1, rs2 } |
        Op::Divu { rd, rs1, rs2 } |
        Op::Rem { rd, rs1, rs2 } |
        Op::Remu { rd, rs1, rs2 } |
        Op::Mulw { rd, rs1, rs2 } |
        Op::Divw { rd, rs1, rs2 } |
        Op::Divuw { rd, rs1, rs2 } |
        Op::Remw { rd, rs1, rs2 } |
        Op::Remuw { rd, rs1, rs2 } =>
            eprint!("{}, {}, {}", register_name(rd), register_name(rs1), register_name(rs2)),
        // CSR instructions store immediates differently.
        Op::Csrrw { rd, rs1, csr } |
        Op::Csrrs { rd, rs1, csr } |
        Op::Csrrc { rd, rs1, csr } =>
            eprint!("{}, #{}, {}", register_name(rd), csr, register_name(rs1)),
        Op::Csrrwi { rd, imm, csr } |
        Op::Csrrsi { rd, imm, csr } |
        Op::Csrrci { rd, imm, csr } =>
            eprint!("{}, #{}, {}", register_name(rd), csr, imm),
        // TODO: For atomic instructions we may want to display their aq, rl arguments?
        Op::LrW { rd, rs1 } |
        Op::LrD { rd, rs1} =>
            eprint!("{}, ({})", register_name(rd), register_name(rs1)),
        Op::ScW { rd, rs1, rs2 } |
        Op::ScD { rd, rs1, rs2 } |
        Op::AmoswapW { rd, rs1, rs2 } |
        Op::AmoswapD { rd, rs1, rs2 } |
        Op::AmoaddW { rd, rs1, rs2 } |
        Op::AmoaddD { rd, rs1, rs2 } |
        Op::AmoxorW { rd, rs1, rs2 } |
        Op::AmoxorD { rd, rs1, rs2 } |
        Op::AmoandW { rd, rs1, rs2 } |
        Op::AmoandD { rd, rs1, rs2 } |
        Op::AmoorW { rd, rs1, rs2 } |
        Op::AmoorD { rd, rs1, rs2 } |
        Op::AmominW { rd, rs1, rs2 } |
        Op::AmominD { rd, rs1, rs2 } |
        Op::AmomaxW { rd, rs1, rs2 } |
        Op::AmomaxD { rd, rs1, rs2 } |
        Op::AmominuW { rd, rs1, rs2 } |
        Op::AmominuD { rd, rs1, rs2 } |
        Op::AmomaxuW { rd, rs1, rs2 } |
        Op::AmomaxuD { rd, rs1, rs2 } =>
            eprint!("{}, {}, ({})", register_name(rd), register_name(rs2), register_name(rs1)),
        // TODO: For floating point arguments we may want to display their r/m arguments?
        Op::Flw { frd, rs1, imm } |
        Op::Fld { frd, rs1, imm } =>
            eprint!("f{}, {}({})", frd, imm, register_name(rs1)),
        Op::Fsw { rs1, frs2, imm } |
        Op::Fsd { rs1, frs2, imm } =>
            eprint!("f{}, {}({})", frs2, imm, register_name(rs1)),
        Op::FaddS { frd, frs1, frs2, ..} |
        Op::FsubS { frd, frs1, frs2, ..} |
        Op::FmulS { frd, frs1, frs2, ..} |
        Op::FdivS { frd, frs1, frs2, ..} |
        Op::FsgnjS { frd, frs1, frs2 } |
        Op::FsgnjnS { frd, frs1, frs2 } |
        Op::FsgnjxS { frd, frs1, frs2 } |
        Op::FminS { frd, frs1, frs2 } |
        Op::FmaxS { frd, frs1, frs2 } |
        Op::FaddD { frd, frs1, frs2, ..} |
        Op::FsubD { frd, frs1, frs2, ..} |
        Op::FmulD { frd, frs1, frs2, ..} |
        Op::FdivD { frd, frs1, frs2, ..} |
        Op::FsgnjD { frd, frs1, frs2 } |
        Op::FsgnjnD { frd, frs1, frs2 } |
        Op::FsgnjxD { frd, frs1, frs2 } |
        Op::FminD { frd, frs1, frs2 } |
        Op::FmaxD { frd, frs1, frs2 } =>
            eprint!("f{}, f{}, f{}", frd, frs1, frs2),
        Op::FsqrtS { frd, frs1, ..} |
        Op::FsqrtD { frd, frs1, ..} |
        Op::FcvtSD { frd, frs1, ..} |
        Op::FcvtDS { frd, frs1, ..} =>
            eprint!("f{}, f{}", frd, frs1),
        Op::FcvtWS { rd, frs1, ..} |
        Op::FcvtWuS { rd, frs1, ..} |
        Op::FcvtLS { rd, frs1, ..} |
        Op::FcvtLuS { rd, frs1, ..} |
        Op::FmvXW { rd, frs1 } |
        Op::FclassS { rd, frs1 } |
        Op::FcvtWD { rd, frs1, ..} |
        Op::FcvtWuD { rd, frs1, ..} |
        Op::FcvtLD { rd, frs1, ..} |
        Op::FcvtLuD { rd, frs1, ..} |
        Op::FmvXD { rd, frs1 } |
        Op::FclassD { rd, frs1 } =>
            eprint!("{}, f{}", register_name(rd), frs1),
        Op::FcvtSW { frd, rs1, ..} |
        Op::FcvtSWu { frd, rs1, ..} |
        Op::FcvtSL { frd, rs1, ..} |
        Op::FcvtSLu { frd, rs1, ..} |
        Op::FmvWX { frd, rs1 } |
        Op::FcvtDW { frd, rs1, ..} |
        Op::FcvtDWu { frd, rs1, ..} |
        Op::FcvtDL { frd, rs1, ..} |
        Op::FcvtDLu { frd, rs1, ..} |
        Op::FmvDX { frd, rs1 } =>
            eprint!("f{}, {}", frd, register_name(rs1)),
        Op::FeqS { rd, frs1, frs2 } |
        Op::FltS { rd, frs1, frs2 } |
        Op::FleS { rd, frs1, frs2 } |
        Op::FeqD { rd, frs1, frs2 } |
        Op::FltD { rd, frs1, frs2 } |
        Op::FleD { rd, frs1, frs2 } =>
            eprint!("{}, f{}, f{}", register_name(rd), frs1, frs2),
        Op::FmaddS { frd, frs1, frs2, frs3, ..} |
        Op::FmsubS { frd, frs1, frs2, frs3, ..} |
        Op::FnmsubS { frd, frs1, frs2, frs3, ..} |
        Op::FnmaddS { frd, frs1, frs2, frs3, ..} |
        Op::FmaddD { frd, frs1, frs2, frs3, ..} |
        Op::FmsubD { frd, frs1, frs2, frs3, ..} |
        Op::FnmsubD { frd, frs1, frs2, frs3, ..} |
        Op::FnmaddD { frd, frs1, frs2, frs3, ..} =>
            eprint!("f{}, f{}, f{}, f{}", frd, frs1, frs2, frs3),
    }
    eprintln!()
}
