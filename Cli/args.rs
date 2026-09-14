use std::path::PathBuf;
use std::process::ExitCode;

use crate::platform::Platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Cranelift,
    Native,
}

impl BackendKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "cranelift" => Some(Self::Cranelift),
            "native" => Some(Self::Native),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Cranelift => "cranelift",
            Self::Native => "native",
        }
    }
}

#[derive(Debug)]
pub enum Command {
    Build {
        input: PathBuf,
        platform: Platform,
        backend: BackendKind,
    },
    Compile {
        input: PathBuf,
        platform: Platform,
        backend: BackendKind,
    },
    Run {
        input: PathBuf,
        platform: Platform,
        backend: BackendKind,
    },
    New {
        app_type: String,
        name: String,
    },
    Backend {
        backend: BackendKind,
    },
    Update,
    Version,
    Help,
}

pub fn parse_args(args: &[String]) -> Result<Command, ExitCode> {
    if args.is_empty() {
        eprintln!("error: no command specified");
        eprintln!();
        eprintln!("Usage:");
        print_compile_usage();
        eprintln!("    Furnace backend <native|cranelift>");
        eprintln!("    Furnace new <APP_TYPE> -n <NAME>");
        eprintln!("    Furnace update");
        eprintln!("Available backends: native, cranelift");
        eprintln!("Helpers:");
        eprintln!("    Furnace -version");
        eprintln!("    Furnace -help");
        return Err(ExitCode::from(2));
    }

    let first = args[0].as_str();

    match first {
        "-version" | "--version" | "-v" | "version" => Ok(Command::Version),
        "-help" | "--help" | "-h" | "help" => Ok(Command::Help),
        "update" | "Update" => Ok(Command::Update),
        "build" | "Build" => {
            if args.len() < 2 {
                eprintln!("error: 'build' requires a .blower project file");
                eprintln!("usage: Furnace build <project>.blower [--windows] [--backend cranelift|native]");
                return Err(ExitCode::from(2));
            }
            let input_path = PathBuf::from(&args[1]);
            validate_blower_extension(&input_path)?;
            let (platform, backend) = parse_compilation_options(&args[2..], false, "build")?;
            Ok(Command::Build {
                input: input_path,
                platform,
                backend,
            })
        }
        "compile" | "Compile" => {
            if args.len() < 2 {
                eprintln!("error: 'compile' requires an input .anvil file");
                eprintln!("usage: Furnace compile <file>.anvil [linux|windows] [--windows] [--backend cranelift|native]");
                return Err(ExitCode::from(2));
            }

            let input_path = PathBuf::from(&args[1]);
            validate_anvil_extension(&input_path)?;

            let (platform, backend) = parse_compilation_options(&args[2..], true, "compile")?;

            Ok(Command::Compile {
                input: input_path,
                platform,
                backend,
            })
        }
        "run" | "Run" => {
            if args.len() < 2 {
                eprintln!("error: 'run' requires an input .anvil file");
                eprintln!(
                    "usage: Furnace run <file>.anvil|<project>.blower [--windows] [--backend cranelift|native]"
                );
                return Err(ExitCode::from(2));
            }

            let input_path = PathBuf::from(&args[1]);
            validate_run_extension(&input_path)?;

            let (platform, backend) = parse_compilation_options(&args[2..], false, "run")?;

            Ok(Command::Run {
                input: input_path,
                platform,
                backend,
            })
        }
        "backend" | "Backend" => {
            if args.len() != 2 {
                eprintln!("error: 'backend' requires a backend type");
                eprintln!("usage: Furnace backend <native|cranelift>");
                eprintln!("available backends: native, cranelift");
                return Err(ExitCode::from(2));
            }

            let backend = parse_backend_kind(&args[1])?;
            Ok(Command::Backend { backend })
        }
        "new" | "New" => {
            if args.len() < 2 {
                eprintln!("error: 'new' requires an application type and a project name");
                eprintln!("usage: Furnace new <APP_TYPE> -n <NAME>");
                return Err(ExitCode::from(2));
            }
            if args.len() != 4 || args[2] != "-n" {
                eprintln!("error: 'new' requires -n <NAME>");
                eprintln!("usage: Furnace new <APP_TYPE> -n <NAME>");
                return Err(ExitCode::from(2));
            }
            if args[3].is_empty() {
                eprintln!("error: project name cannot be empty");
                return Err(ExitCode::from(2));
            }

            Ok(Command::New {
                app_type: args[1].clone(),
                name: args[3].clone(),
            })
        }
        unknown => {
            eprintln!("error: unknown command '{}'", unknown);
            eprintln!();
            eprintln!("Usage:");
            print_compile_usage();
            eprintln!("    Furnace backend <native|cranelift>");
            eprintln!("    Furnace new <APP_TYPE> -n <NAME>");
            eprintln!("Available backends: native, cranelift");
            eprintln!("    Furnace -version");
            eprintln!("    Furnace -help");
            Err(ExitCode::from(2))
        }
    }
}

fn validate_blower_extension(path: &PathBuf) -> Result<(), ExitCode> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("blower") => Ok(()),
        _ => {
            eprintln!("error: furnace build requires a .blower project file");
            Err(ExitCode::from(2))
        }
    }
}

fn parse_compilation_options(
    remaining_args: &[String],
    allow_platform: bool,
    command: &str,
) -> Result<(Platform, BackendKind), ExitCode> {
    let mut backend = None;
    let mut positional_platform = None;
    let mut windows_flag = false;
    let mut i = 0;
    while i < remaining_args.len() {
        if remaining_args[i] == "--backend" || remaining_args[i] == "-b" {
            if i + 1 >= remaining_args.len() {
                if command == "build" {
                    eprintln!(
                        "error: unsupported furnace build argument '{}'; expected --backend native|cranelift or --windows",
                        remaining_args[i]
                    );
                } else {
                    eprintln!("error: missing argument for --backend");
                }
                return Err(ExitCode::from(2));
            }
            backend = Some(parse_backend_kind(&remaining_args[i + 1])?);
            i += 2;
        } else if remaining_args[i] == "--windows" {
            windows_flag = true;
            i += 1;
        } else if allow_platform
            && !remaining_args[i].starts_with('-')
            && positional_platform.is_none()
        {
            positional_platform = Some(Platform::parse(&remaining_args[i]).map_err(|message| {
                eprintln!("error: {}", message);
                ExitCode::from(2)
            })?);
            i += 1;
        } else {
            eprintln!(
                "error: unsupported furnace {} argument '{}'",
                command, remaining_args[i]
            );
            return Err(ExitCode::from(2));
        }
    }

    if windows_flag && positional_platform == Some(Platform::Linux) {
        eprintln!(
            "error: conflicting targets: positional 'linux' cannot be combined with --windows"
        );
        return Err(ExitCode::from(2));
    }
    let platform = if windows_flag {
        Platform::Windows
    } else {
        positional_platform.unwrap_or(Platform::Linux)
    };
    let backend = if platform == Platform::Windows && backend.is_none() {
        BackendKind::Native
    } else {
        resolve_backend(backend)?
    };
    if platform == Platform::Windows && backend != BackendKind::Native {
        eprintln!("error: Windows targeting requires the native backend; Cranelift Windows linking is not supported");
        return Err(ExitCode::from(2));
    }
    Ok((platform, backend))
}

fn print_compile_usage() {
    eprintln!("    Furnace build <project>.blower [--windows] [--backend cranelift|native]");
    eprintln!(
        "    Furnace compile <file>.anvil [linux|windows] [--windows] [--backend cranelift|native]"
    );
    eprintln!(
        "    Furnace run <file>.anvil|<project>.blower [--windows] [--backend cranelift|native]"
    );
}

fn resolve_backend(explicit: Option<BackendKind>) -> Result<BackendKind, ExitCode> {
    crate::config::resolve_backend(explicit).map_err(|message| {
        eprintln!("error: {}", message);
        ExitCode::from(1)
    })
}

fn parse_backend_kind(value: &str) -> Result<BackendKind, ExitCode> {
    BackendKind::parse(value).ok_or_else(|| {
        eprintln!(
            "error: unknown backend '{}'. Available backends: native, cranelift",
            value
        );
        ExitCode::from(2)
    })
}

fn validate_anvil_extension(path: &PathBuf) -> Result<(), ExitCode> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("anvil") => Ok(()),
        _ => {
            eprintln!(
                "error: input file '{}' must have a .anvil extension",
                path.display()
            );
            Err(ExitCode::from(2))
        }
    }
}

fn validate_run_extension(path: &PathBuf) -> Result<(), ExitCode> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("anvil" | "blower") => Ok(()),
        _ => {
            eprintln!(
                "error: run input '{}' must have an .anvil or .blower extension",
                path.display()
            );
            Err(ExitCode::from(2))
        }
    }
}
