use std::process::{Command, Stdio};

use crate::Result;

/// Captured output of a blocking command run.
#[derive(Debug, Clone, Default)]
pub struct CmdOut {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

/// The only abstraction this crate allows: every external command goes through it.
pub trait CmdRunner: Send {
    /// Blocking run; argv[0] is the executable, captures stdout/stderr.
    fn run(&mut self, args: &[&str]) -> Result<CmdOut>;
    /// Spawn a background process (stdio to /dev/null); returns its PID.
    fn run_detached(&mut self, args: &[&str]) -> Result<u32>;
}

/// Runs real processes via std.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealRunner;

impl CmdRunner for RealRunner {
    fn run(&mut self, args: &[&str]) -> Result<CmdOut> {
        let (prog, rest) = args.split_first().unwrap_or((&"", &[]));
        let output = if rest.is_empty() {
            Command::new(prog).output()
        } else {
            Command::new(prog).args(rest).output()
        }?;
        Ok(CmdOut {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn run_detached(&mut self, args: &[&str]) -> Result<u32> {
        let (prog, rest) = args.split_first().unwrap_or((&"", &[]));
        let mut cmd = Command::new(prog);
        cmd.args(rest)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = cmd.spawn()?;
        Ok(child.id())
    }
}
