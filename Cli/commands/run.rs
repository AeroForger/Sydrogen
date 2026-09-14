use std::path::Path;
use std::process::{Command, ExitCode};

use crate::args::BackendKind;
use crate::platform::Platform;

pub fn execute(input: &Path, platform: Platform, backend: BackendKind) -> ExitCode {
    if platform != host_platform() {
        eprintln!("error: furnace run cannot execute a {} target on this host; use furnace compile or furnace build for cross-compilation", platform);
        return ExitCode::from(1);
    }
    let program = match load_program(input) {
        Ok(program) => program,
        Err(error) => {
            eprintln!("{}", error);
            return ExitCode::from(1);
        }
    };

    if let Err(e) = furnace::semantic::analyze(&program) {
        eprintln!("{}", e);
        return ExitCode::from(1);
    }

    let pid = std::process::id();
    let temp_dir = std::env::temp_dir();
    let exe_path = temp_dir.join(format!(
        "furnace_run_{}_{}.exe",
        pid,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    match backend {
        BackendKind::Native => {
            let executable_bytes =
                match furnace::backend::compile_native(&program, platform.native_target()) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        eprintln!("{}", e);
                        return ExitCode::from(1);
                    }
                };

            if let Err(e) = std::fs::write(&exe_path, executable_bytes) {
                eprintln!("error: cannot write temporary executable: {}", e);
                return ExitCode::from(1);
            }

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = std::fs::metadata(&exe_path) {
                    let mut perms = metadata.permissions();
                    perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(&exe_path, perms);
                }
            }
        }
        BackendKind::Cranelift => {
            let obj_path = temp_dir.join(format!(
                "furnace_run_{}_{}.o",
                pid,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));

            if let Err(e) = furnace::codegen::compile(&program, &obj_path, true) {
                eprintln!("{}", e);
                return ExitCode::from(1);
            }

            let runtime_path = obj_path.with_extension("runtime.c");
            if let Err(e) = std::fs::write(&runtime_path, furnace::codegen::RUNTIME_SUPPORT_C) {
                let _ = std::fs::remove_file(&obj_path);
                eprintln!("error: cannot write runtime support: {}", e);
                return ExitCode::from(1);
            }

            let mut linker = Command::new(platform.linker_name());
            linker
                .arg(&obj_path)
                .arg(&runtime_path)
                .arg("-o")
                .arg(&exe_path);
            for flag in platform.default_linker_flags() {
                linker.arg(flag);
            }

            let link_status = match linker.status() {
                Ok(status) => status,
                Err(e) => {
                    let _ = std::fs::remove_file(&obj_path);
                    let _ = std::fs::remove_file(&runtime_path);
                    eprintln!(
                        "error: cannot invoke linker '{}': {}",
                        platform.linker_name(),
                        e
                    );
                    return ExitCode::from(1);
                }
            };

            let _ = std::fs::remove_file(&obj_path);
            let _ = std::fs::remove_file(&runtime_path);

            if !link_status.success() {
                eprintln!("error: linker exited with status {}", link_status);
                return ExitCode::from(1);
            }
        }
    }

    let run_status = match Command::new(&exe_path)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
    {
        Ok(status) => status,
        Err(e) => {
            let _ = std::fs::remove_file(&exe_path);
            eprintln!("error: failed to execute compiled program: {}", e);
            return ExitCode::from(1);
        }
    };

    let _ = std::fs::remove_file(&exe_path);

    if let Some(code) = run_status.code() {
        ExitCode::from(code as u8)
    } else {
        ExitCode::from(1)
    }
}

fn host_platform() -> Platform {
    #[cfg(windows)]
    {
        Platform::Windows
    }
    #[cfg(not(windows))]
    {
        Platform::Linux
    }
}

fn load_program(input: &Path) -> Result<furnace::ast::Program, String> {
    if input.extension().and_then(|extension| extension.to_str()) == Some("blower") {
        return furnace::project::Project::load(input)
            .and_then(|project| project.resolved_program())
            .map_err(|error| error.to_string());
    }
    furnace::imports::load_standalone(input).map_err(|error| error.to_string())
}
