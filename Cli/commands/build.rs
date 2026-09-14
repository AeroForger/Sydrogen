use std::path::Path;
use std::process::{Command, ExitCode};

use crate::args::BackendKind;
use crate::platform::Platform;

pub fn execute(input: &Path, platform: Platform, backend: BackendKind) -> ExitCode {
    println!("Building project {}...", input.display());
    let project = match furnace::project::Project::load(input) {
        Ok(project) => project,
        Err(error) => {
            eprintln!("{}", error);
            return ExitCode::from(1);
        }
    };
    let program = match project.resolved_program() {
        Ok(program) => program,
        Err(error) => {
            eprintln!("{}", error);
            return ExitCode::from(1);
        }
    };
    if let Err(error) = furnace::semantic::analyze(&program) {
        eprintln!("{}", error);
        return ExitCode::from(1);
    }

    let output = platform.output_path(project.output_path());
    let build_dir = output.parent().expect("project output always has a parent");
    if let Err(error) = std::fs::create_dir_all(build_dir) {
        eprintln!(
            "error: cannot create build directory '{}': {}",
            build_dir.display(),
            error
        );
        return ExitCode::from(1);
    }

    let result = match backend {
        BackendKind::Native => build_native(&program, &output, platform),
        BackendKind::Cranelift => build_cranelift(&program, &output, build_dir, platform),
    };
    if let Err(error) = result {
        eprintln!("{}", error);
        return ExitCode::from(1);
    }

    println!("Build successful!");
    println!("Backend: {}", backend.name());
    println!("Output: {}", output.display());
    ExitCode::SUCCESS
}

fn build_native(
    program: &furnace::ast::Program,
    output: &Path,
    platform: Platform,
) -> Result<(), String> {
    let bytes = furnace::backend::compile_native(program, platform.native_target())
        .map_err(|error| error.to_string())?;
    std::fs::write(output, bytes).map_err(|error| {
        format!(
            "error: cannot write executable '{}': {}",
            output.display(),
            error
        )
    })?;
    if platform == Platform::Linux {
        make_executable(output);
    }
    Ok(())
}

fn build_cranelift(
    program: &furnace::ast::Program,
    output: &Path,
    build_dir: &Path,
    platform: Platform,
) -> Result<(), String> {
    let temporary = tempfile::Builder::new()
        .prefix(".furnace-")
        .tempdir_in(build_dir)
        .map_err(|error| format!("error: cannot create temporary build directory: {}", error))?;
    let object = temporary.path().join("project.o");
    let runtime = temporary.path().join("runtime.c");
    furnace::codegen::compile(program, &object, true).map_err(|error| error.to_string())?;
    std::fs::write(&runtime, furnace::codegen::RUNTIME_SUPPORT_C)
        .map_err(|error| format!("error: cannot write runtime support: {}", error))?;

    println!("Linking...");
    let mut linker = Command::new(platform.linker_name());
    linker.arg(&object).arg(&runtime).arg("-o").arg(output);
    for flag in platform.default_linker_flags() {
        linker.arg(flag);
    }
    let status = linker.status().map_err(|error| {
        format!(
            "error: cannot invoke linker '{}': {}",
            platform.linker_name(),
            error
        )
    })?;
    if !status.success() {
        return Err(format!("error: linker exited with status {}", status));
    }
    make_executable(output);
    Ok(())
}

fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o755);
            let _ = std::fs::set_permissions(path, permissions);
        }
    }
}
