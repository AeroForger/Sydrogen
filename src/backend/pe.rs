use std::collections::HashMap;

use crate::errors::{ForgeError, ForgeResult};

pub const IMAGE_BASE: u64 = 0x0000_0001_4000_0000;
pub const SECTION_RVA: u32 = 0x1000;
pub const FILE_ALIGNMENT: u32 = 0x200;
pub const SECTION_ALIGNMENT: u32 = 0x1000;
pub const HEADERS_SIZE: u32 = 0x200;
pub const STARTUP_STUB_SIZE: usize = 14;

const PE_OFFSET: usize = 0x80;

pub fn generate_pe32_plus_executable(
    user_code: &[u8],
    func_offsets: &HashMap<String, usize>,
) -> ForgeResult<Vec<u8>> {
    let main_offset = *func_offsets.get("Main").ok_or_else(|| {
        ForgeError::codegen("Cannot generate PE32+ executable without a 'Main' function")
    })?;

    // Windows x64 requires 32 bytes of shadow space and 16-byte alignment at
    // call sites. The process loader terminates the process when its entry
    // routine returns, so Main's RAX value is returned directly.
    let mut stub = Vec::with_capacity(STARTUP_STUB_SIZE);
    stub.extend_from_slice(&[0x48, 0x83, 0xEC, 0x28]); // sub rsp, 40
    stub.push(0xE8); // call Main
    let main_relative = STARTUP_STUB_SIZE as i64 + main_offset as i64 - 9;
    let main_relative = i32::try_from(main_relative)
        .map_err(|_| ForgeError::codegen("Windows entry point is too far from Main"))?;
    stub.extend_from_slice(&main_relative.to_le_bytes());
    stub.extend_from_slice(&[0x48, 0x83, 0xC4, 0x28]); // add rsp, 40
    stub.push(0xC3); // ret
    debug_assert_eq!(stub.len(), STARTUP_STUB_SIZE);

    let virtual_size = u32::try_from(stub.len() + user_code.len())
        .map_err(|_| ForgeError::codegen("Windows native code exceeds PE32+ section limits"))?;
    let raw_size = align_up(virtual_size, FILE_ALIGNMENT)?;
    let image_size = align_up(
        SECTION_RVA
            .checked_add(virtual_size)
            .ok_or_else(|| ForgeError::codegen("Windows PE32+ image size overflow"))?,
        SECTION_ALIGNMENT,
    )?;

    let mut pe = vec![0u8; HEADERS_SIZE as usize];

    // DOS header and stub. Only e_magic and e_lfanew are required by the PE loader.
    pe[0..2].copy_from_slice(b"MZ");
    pe[0x3c..0x40].copy_from_slice(&(PE_OFFSET as u32).to_le_bytes());
    let dos_message = b"This program cannot be run in DOS mode.\r\n$";
    pe[0x40..0x40 + dos_message.len()].copy_from_slice(dos_message);

    let mut cursor = PE_OFFSET;
    write(&mut pe, &mut cursor, b"PE\0\0");

    // COFF header.
    write_u16(&mut pe, &mut cursor, 0x8664); // IMAGE_FILE_MACHINE_AMD64
    write_u16(&mut pe, &mut cursor, 1); // one section
    write_u32(&mut pe, &mut cursor, 0); // reproducible timestamp
    write_u32(&mut pe, &mut cursor, 0); // symbol table
    write_u32(&mut pe, &mut cursor, 0); // symbols
    write_u16(&mut pe, &mut cursor, 240); // PE32+ optional header size
    write_u16(&mut pe, &mut cursor, 0x0023); // executable, fixed-base, large-address-aware

    // PE32+ optional header.
    write_u16(&mut pe, &mut cursor, 0x020b); // PE32+ magic
    write(&mut pe, &mut cursor, &[1, 0]); // linker version
    write_u32(&mut pe, &mut cursor, raw_size); // SizeOfCode
    write_u32(&mut pe, &mut cursor, 0); // initialized data
    write_u32(&mut pe, &mut cursor, 0); // uninitialized data
    write_u32(&mut pe, &mut cursor, SECTION_RVA); // AddressOfEntryPoint
    write_u32(&mut pe, &mut cursor, SECTION_RVA); // BaseOfCode
    write_u64(&mut pe, &mut cursor, IMAGE_BASE);
    write_u32(&mut pe, &mut cursor, SECTION_ALIGNMENT);
    write_u32(&mut pe, &mut cursor, FILE_ALIGNMENT);
    write_u16(&mut pe, &mut cursor, 6); // Windows 6.0+
    write_u16(&mut pe, &mut cursor, 0);
    write_u16(&mut pe, &mut cursor, 0); // image version
    write_u16(&mut pe, &mut cursor, 0);
    write_u16(&mut pe, &mut cursor, 6); // subsystem version
    write_u16(&mut pe, &mut cursor, 0);
    write_u32(&mut pe, &mut cursor, 0); // Win32VersionValue
    write_u32(&mut pe, &mut cursor, image_size);
    write_u32(&mut pe, &mut cursor, HEADERS_SIZE);
    write_u32(&mut pe, &mut cursor, 0); // checksum
    write_u16(&mut pe, &mut cursor, 3); // Windows CUI
    write_u16(&mut pe, &mut cursor, 0x0100); // NX compatible
    write_u64(&mut pe, &mut cursor, 0x10_0000); // stack reserve
    write_u64(&mut pe, &mut cursor, 0x1000); // stack commit
    write_u64(&mut pe, &mut cursor, 0x10_0000); // heap reserve
    write_u64(&mut pe, &mut cursor, 0x1000); // heap commit
    write_u32(&mut pe, &mut cursor, 0); // loader flags
    write_u32(&mut pe, &mut cursor, 16); // data-directory count
    for _ in 0..16 {
        write_u64(&mut pe, &mut cursor, 0);
    }

    // Single executable/readable .text section.
    write(&mut pe, &mut cursor, b".text\0\0\0");
    write_u32(&mut pe, &mut cursor, virtual_size);
    write_u32(&mut pe, &mut cursor, SECTION_RVA);
    write_u32(&mut pe, &mut cursor, raw_size);
    write_u32(&mut pe, &mut cursor, HEADERS_SIZE);
    write_u32(&mut pe, &mut cursor, 0); // relocations
    write_u32(&mut pe, &mut cursor, 0); // line numbers
    write_u16(&mut pe, &mut cursor, 0);
    write_u16(&mut pe, &mut cursor, 0);
    write_u32(&mut pe, &mut cursor, 0x6000_0020); // code, execute, read

    debug_assert!(cursor <= HEADERS_SIZE as usize);
    pe.extend_from_slice(&stub);
    pe.extend_from_slice(user_code);
    pe.resize((HEADERS_SIZE + raw_size) as usize, 0);
    Ok(pe)
}

fn align_up(value: u32, alignment: u32) -> ForgeResult<u32> {
    value
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
        .ok_or_else(|| ForgeError::codegen("Windows PE32+ alignment overflow"))
}

fn write(output: &mut [u8], cursor: &mut usize, bytes: &[u8]) {
    output[*cursor..*cursor + bytes.len()].copy_from_slice(bytes);
    *cursor += bytes.len();
}

fn write_u16(output: &mut [u8], cursor: &mut usize, value: u16) {
    write(output, cursor, &value.to_le_bytes());
}

fn write_u32(output: &mut [u8], cursor: &mut usize, value: u32) {
    write(output, cursor, &value.to_le_bytes());
}

fn write_u64(output: &mut [u8], cursor: &mut usize, value: u64) {
    write(output, cursor, &value.to_le_bytes());
}
