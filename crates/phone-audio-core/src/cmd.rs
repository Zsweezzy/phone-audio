use crate::Result;
use std::process::{Command, Stdio};
#[derive(Debug, Clone, Default)]
pub struct CmdOut {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}
pub trait CmdRunner: Send {
    fn run(&mut self, args: &[&str]) -> Result<CmdOut>;
    fn run_detached(&mut self, args: &[&str]) -> Result<u32>;
    fn box_clone(&self) -> Box<dyn CmdRunner>;
}
#[derive(Debug, Default, Clone, Copy)]
pub struct RealRunner;
impl CmdRunner for RealRunner {
    fn run(&mut self, args: &[&str]) -> Result<CmdOut> {
        let (prog, rest) = args.split_first().unwrap_or((&"", &[]));
        let output = Command::new(prog).args(rest).output()?;
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
    fn box_clone(&self) -> Box<dyn CmdRunner> {
        Box::new(*self)
    }
}
