use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use furnace::backend::{compile_native, compile_to_elf, compile_to_pe, Target};
use furnace::parser::parse_program;
use furnace::semantic::analyze;

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn simple_program() -> furnace::ast::Program {
    let program = parse_program("Open Int Main() { Return 42; }").unwrap();
    analyze(&program).unwrap();
    program
}

fn run(command: &mut Command) -> Output {
    command.output().unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn write_source(directory: &Path, source: &str) {
    fs::write(directory.join("main.anvil"), source).unwrap();
}

#[test]
fn pe32_plus_headers_identify_x86_64_console_executable() {
    let pe = compile_to_pe(&simple_program()).unwrap();
    assert_eq!(&pe[0..2], b"MZ");
    let pe_offset = u32_at(&pe, 0x3c) as usize;
    assert_eq!(pe_offset, 0x80);
    assert_eq!(&pe[pe_offset..pe_offset + 4], b"PE\0\0");

    let coff = pe_offset + 4;
    assert_eq!(u16_at(&pe, coff), 0x8664); // IMAGE_FILE_MACHINE_AMD64
    assert_eq!(u16_at(&pe, coff + 2), 1);
    assert_eq!(u16_at(&pe, coff + 16), 240);
    assert_eq!(u16_at(&pe, coff + 18) & 0x0002, 0x0002);

    let optional = coff + 20;
    assert_eq!(u16_at(&pe, optional), 0x020b); // PE32+
    assert_eq!(u32_at(&pe, optional + 16), 0x1000); // entry RVA
    assert_eq!(u32_at(&pe, optional + 20), 0x1000); // code RVA
    assert_eq!(u64_at(&pe, optional + 24), 0x0000_0001_4000_0000);
    assert_eq!(u32_at(&pe, optional + 32), 0x1000);
    assert_eq!(u32_at(&pe, optional + 36), 0x200);
    assert_eq!(u32_at(&pe, optional + 60), 0x200);
    assert_eq!(u16_at(&pe, optional + 68), 3); // IMAGE_SUBSYSTEM_WINDOWS_CUI
    assert_eq!(u32_at(&pe, optional + 108), 16);
}

#[test]
fn pe_text_section_is_aligned_and_entry_calls_main() {
    let pe = compile_to_pe(&simple_program()).unwrap();
    let section = 0x80 + 4 + 20 + 240;
    assert_eq!(&pe[section..section + 8], b".text\0\0\0");
    let virtual_size = u32_at(&pe, section + 8);
    assert!(virtual_size > 14);
    assert_eq!(u32_at(&pe, section + 12), 0x1000);
    let raw_size = u32_at(&pe, section + 16);
    let raw_offset = u32_at(&pe, section + 20);
    assert_eq!(raw_size % 0x200, 0);
    assert_eq!(raw_offset, 0x200);
    assert_eq!(pe.len(), (raw_offset + raw_size) as usize);
    assert_eq!(u32_at(&pe, section + 36), 0x6000_0020);

    let entry = raw_offset as usize;
    assert_eq!(&pe[entry..entry + 5], &[0x48, 0x83, 0xec, 0x28, 0xe8]);
    let displacement = i32::from_le_bytes(pe[entry + 5..entry + 9].try_into().unwrap());
    assert_eq!(
        (entry + 9) as i64 + displacement as i64,
        (entry + 14) as i64
    );
    assert_eq!(&pe[entry + 9..entry + 14], &[0x48, 0x83, 0xc4, 0x28, 0xc3]);
}

#[test]
fn linux_target_remains_elf64_and_default_wrapper_matches() {
    let program = simple_program();
    let legacy = compile_to_elf(&program).unwrap();
    let targeted = compile_native(&program, Target::Linux).unwrap();
    assert_eq!(legacy, targeted);
    assert_eq!(&legacy[..5], &[0x7f, b'E', b'L', b'F', 2]);
    assert_eq!(u16_at(&legacy, 18), 0x3e);
}

#[test]
fn invalid_windows_programs_return_target_specific_errors() {
    let printed = parse_program("Open Nunction Main() { Print(1); }").unwrap();
    let error = compile_to_pe(&printed).unwrap_err().to_string();
    assert!(error.contains("Windows native compilation"));
    assert!(error.contains("Print"));

    let typed = parse_program("Open Float Main() { Return 1.5; }").unwrap();
    let error = compile_to_pe(&typed).unwrap_err().to_string();
    assert!(error.contains("does not yet support floats"));

    let error = furnace::backend::pe::generate_pe32_plus_executable(&[], &HashMap::new())
        .unwrap_err()
        .to_string();
    assert!(error.contains("without a 'Main' function"));
}

#[test]
fn compile_accepts_windows_flag_and_positional_target_and_writes_exe() {
    for target_args in [&["--windows"][..], &["windows"][..]] {
        let temp = tempfile::tempdir().unwrap();
        write_source(temp.path(), "Open Int Main() { Return 7; }");
        let output = run(Command::new(env!("CARGO_BIN_EXE_furnace"))
            .current_dir(temp.path())
            .args(["compile", "main.anvil"])
            .args(target_args.iter().copied()));
        assert!(output.status.success(), "{}", stderr(&output));
        let executable = fs::read(temp.path().join("main.exe")).unwrap();
        assert_eq!(&executable[..2], b"MZ");
        assert!(!temp.path().join("main").exists());
    }
}

#[test]
fn windows_flag_is_available_to_build_and_run_and_reaches_native_backend() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir(temp.path().join("src")).unwrap();
    fs::write(
        temp.path().join("src/Main.anvil"),
        "Open Int Main() { Return 3; }",
    )
    .unwrap();
    fs::write(
        temp.path().join("app.blower"),
        "Project { Name=\"WindowsApp\"; } Files { location=\"src/*.anvil\"; }",
    )
    .unwrap();
    let built = run(Command::new(env!("CARGO_BIN_EXE_furnace")).args([
        "build",
        temp.path().join("app.blower").to_str().unwrap(),
        "--windows",
    ]));
    assert!(built.status.success(), "{}", stderr(&built));
    assert_eq!(
        &fs::read(temp.path().join("build/WindowsApp.exe")).unwrap()[..2],
        b"MZ"
    );

    write_source(temp.path(), "Open Int Main() { Return 0; }");
    let run_output = run(Command::new(env!("CARGO_BIN_EXE_furnace")).args([
        "run",
        temp.path().join("main.anvil").to_str().unwrap(),
        "--windows",
    ]));
    #[cfg(not(windows))]
    {
        assert!(!run_output.status.success());
        assert!(stderr(&run_output).contains("cannot execute a windows target on this host"));
    }
}

#[test]
fn windows_target_never_silently_uses_cranelift_or_elf() {
    let temp = tempfile::tempdir().unwrap();
    write_source(temp.path(), "Open Int Main() { Return 7; }");
    let output = run(Command::new(env!("CARGO_BIN_EXE_furnace"))
        .current_dir(temp.path())
        .args([
            "compile",
            "main.anvil",
            "--windows",
            "--backend",
            "cranelift",
        ]));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("requires the native backend"));
    assert!(!temp.path().join("main").exists());
    assert!(!temp.path().join("main.exe").exists());
}
