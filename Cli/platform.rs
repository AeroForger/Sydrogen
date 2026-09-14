use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    Windows,
}

impl Platform {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "linux" => Ok(Platform::Linux),
            "windows" => Ok(Platform::Windows),
            other => Err(format!(
                "unsupported target platform '{}'. Currently supported platforms: linux, windows",
                other
            )),
        }
    }

    pub fn linker_name(&self) -> &'static str {
        match self {
            Platform::Linux => "cc",
            Platform::Windows => "",
        }
    }

    pub fn default_linker_flags(&self) -> &'static [&'static str] {
        match self {
            Platform::Linux => &["-lm"],
            Platform::Windows => &[],
        }
    }

    pub fn native_target(self) -> furnace::backend::Target {
        match self {
            Self::Linux => furnace::backend::Target::Linux,
            Self::Windows => furnace::backend::Target::Windows,
        }
    }

    pub fn output_path(self, path: std::path::PathBuf) -> std::path::PathBuf {
        match self.native_target().executable_extension() {
            Some(extension) => path.with_extension(extension),
            None => path,
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Platform::Linux => write!(f, "linux"),
            Platform::Windows => write!(f, "windows"),
        }
    }
}
