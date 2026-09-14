use std::process::ExitCode;

pub fn execute() -> ExitCode {
    println!("Furnace {}", furnace::VERSION);
    println!();
    println!("Usage:");
    println!("    Furnace build <project>.blower [--windows] [--backend native|cranelift]");
    println!(
        "    Furnace compile <file>.anvil [linux|windows] [--windows] [--backend native|cranelift]"
    );
    println!(
        "    Furnace run <file>.anvil|<project>.blower [--windows] [--backend native|cranelift]"
    );
    println!("    Furnace backend <native|cranelift>");
    println!("    Furnace new <APP_TYPE> -n <NAME>");
    println!("    Furnace update");
    println!();
    println!("Application types:");
    println!("    console");
    println!("    Furnace -version");
    println!("    Furnace -help");
    println!();
    println!("Available backends:");
    println!("    native: direct x86-64 ELF64 or PE32+ executable");
    println!("    cranelift: native object file linked with cc");
    ExitCode::SUCCESS
}
