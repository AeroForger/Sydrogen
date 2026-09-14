use std::collections::HashMap;

use crate::errors::{ForgeError, ForgeResult};

pub const STARTUP_STUB_SIZE: usize = 43;

pub fn generate_elf64_executable(
    user_code: &[u8],
    func_offsets: &HashMap<String, usize>,
) -> ForgeResult<Vec<u8>> {
    let main_offset = *func_offsets.get("Main").ok_or_else(|| {
        ForgeError::codegen("Cannot generate ELF executable without a 'Main' function")
    })?;
    let uncaught_offset = *func_offsets
        .get("__sydrogen_uncaught_error")
        .ok_or_else(|| {
            ForgeError::codegen("Cannot generate ELF executable without the error runtime")
        })?;

    // Startup stub (_start): call Main, branch to the uncaught-error formatter
    // when RDX is nonzero, then terminate through sys_exit.
    let mut stub = Vec::new();
    stub.push(0xE8); // call rel32
    let target_rel = (STARTUP_STUB_SIZE as i64 + main_offset as i64) - 5;
    stub.extend_from_slice(&(target_rel as i32).to_le_bytes());
    stub.extend_from_slice(&[0x48, 0x85, 0xD2]); // test rdx, rdx
    stub.extend_from_slice(&[0x0F, 0x85]); // jne uncaught
    stub.extend_from_slice(&12i32.to_le_bytes());
    stub.extend_from_slice(&[0x48, 0x89, 0xC7]); // mov rdi, rax
    stub.extend_from_slice(&[0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00]); // mov rax, 60
    stub.extend_from_slice(&[0x0F, 0x05]); // syscall
    stub.push(0xE8); // call uncaught formatter
    let uncaught_rel = STARTUP_STUB_SIZE as i64 + uncaught_offset as i64 - 31;
    stub.extend_from_slice(&(uncaught_rel as i32).to_le_bytes());
    stub.extend_from_slice(&[0x48, 0x89, 0xC7]); // mov rdi, rax
    stub.extend_from_slice(&[0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00]); // mov rax, 60
    stub.extend_from_slice(&[0x0F, 0x05]); // syscall
    debug_assert_eq!(stub.len(), STARTUP_STUB_SIZE);

    let header_size: u64 = 128; // 64 bytes Ehdr + 56 bytes Phdr + 8 bytes pad
    let total_code_size = (stub.len() + user_code.len()) as u64;
    let file_size = header_size + total_code_size;

    let base_vaddr: u64 = 0x400000;
    let entry_vaddr = base_vaddr + header_size;

    let mut elf = Vec::new();

    // 1. ELF Header (64 bytes)
    // e_ident
    elf.extend_from_slice(&[0x7F, b'E', b'L', b'F']); // magic
    elf.push(2); // ELFCLASS64
    elf.push(1); // ELFDATA2LSB (little endian)
    elf.push(1); // EV_CURRENT
    elf.push(0); // ELFOSABI_NONE
    elf.extend_from_slice(&[0u8; 8]); // padding

    elf.extend_from_slice(&2u16.to_le_bytes()); // e_type: ET_EXEC
    elf.extend_from_slice(&0x3Eu16.to_le_bytes()); // e_machine: EM_X86_64
    elf.extend_from_slice(&1u32.to_le_bytes()); // e_version: 1
    elf.extend_from_slice(&entry_vaddr.to_le_bytes()); // e_entry
    elf.extend_from_slice(&64u64.to_le_bytes()); // e_phoff: immediately after Ehdr (64)
    elf.extend_from_slice(&0u64.to_le_bytes()); // e_shoff: 0
    elf.extend_from_slice(&0u32.to_le_bytes()); // e_flags: 0
    elf.extend_from_slice(&64u16.to_le_bytes()); // e_ehsize: 64
    elf.extend_from_slice(&56u16.to_le_bytes()); // e_phentsize: 56
    elf.extend_from_slice(&1u16.to_le_bytes()); // e_phnum: 1
    elf.extend_from_slice(&64u16.to_le_bytes()); // e_shentsize: 64
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shnum: 0
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shstrndx: 0

    debug_assert_eq!(elf.len(), 64);

    // 2. Program Header (56 bytes)
    elf.extend_from_slice(&1u32.to_le_bytes()); // p_type: PT_LOAD (1)
    elf.extend_from_slice(&7u32.to_le_bytes()); // p_flags: PF_R | PF_W | PF_X (7)
    elf.extend_from_slice(&0u64.to_le_bytes()); // p_offset: 0
    elf.extend_from_slice(&base_vaddr.to_le_bytes()); // p_vaddr: 0x400000
    elf.extend_from_slice(&base_vaddr.to_le_bytes()); // p_paddr: 0x400000
    elf.extend_from_slice(&file_size.to_le_bytes()); // p_filesz
    elf.extend_from_slice(&file_size.to_le_bytes()); // p_memsz
    elf.extend_from_slice(&0x1000u64.to_le_bytes()); // p_align: 4096

    debug_assert_eq!(elf.len(), 120);

    // Padding to 128 bytes
    elf.extend_from_slice(&[0u8; 8]);
    debug_assert_eq!(elf.len(), 128);

    // 3. Code Payload
    elf.extend_from_slice(&stub);
    elf.extend_from_slice(user_code);

    Ok(elf)
}
