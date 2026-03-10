use nix::sys::ptrace;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{execvp, fork, ForkResult, Pid};
use std::ffi::CString;

pub struct ProcessLauncher {
    pub pid: i32,
}

impl ProcessLauncher {
    /// Launch a program under ptrace control using fork/exec.
    /// The child calls PT_TRACE_ME before exec, so the kernel stops it
    /// at the first instruction -- no race condition, no missed instructions.
    pub fn launch(program: &str, args: &[String]) -> Result<Self, String> {
        let c_program =
            CString::new(program).map_err(|e| format!("Invalid program name: {}", e))?;

        let mut c_args: Vec<CString> = vec![c_program.clone()];
        for arg in args {
            c_args
                .push(CString::new(arg.as_str()).map_err(|e| format!("Invalid argument: {}", e))?);
        }

        match unsafe { fork() } {
            Ok(ForkResult::Child) => {
                // Child: request tracing, then exec.
                // PT_TRACE_ME causes the kernel to stop us on exec.
                ptrace::traceme().unwrap_or_else(|e| {
                    eprintln!("ptrace(PT_TRACE_ME) failed: {}", e);
                    std::process::exit(127);
                });
                let _err = execvp(&c_program, &c_args);
                // execvp only returns on error
                eprintln!("exec failed: {}", _err.unwrap_err());
                std::process::exit(127);
            }
            Ok(ForkResult::Parent { child }) => {
                // Wait for the child to stop at exec
                match waitpid(child, None) {
                    Ok(WaitStatus::Stopped(_, _)) => Ok(Self {
                        pid: child.as_raw(),
                    }),
                    Ok(WaitStatus::Exited(_, code)) => {
                        Err(format!("Child exited immediately with code {}", code))
                    }
                    Ok(status) => Err(format!("Unexpected wait status after fork: {:?}", status)),
                    Err(e) => Err(format!("waitpid failed: {}", e)),
                }
            }
            Err(e) => Err(format!("fork failed: {}", e)),
        }
    }

    /// Attach to an already-running process via ptrace.
    /// The process will be stopped (SIGSTOP) after this returns.
    pub fn attach(pid: i32) -> Result<Self, String> {
        let nix_pid = Pid::from_raw(pid);
        ptrace::attach(nix_pid)
            .map_err(|e| format!("ptrace(PT_ATTACH) failed: {} (need sudo?)", e))?;

        match waitpid(nix_pid, None) {
            Ok(WaitStatus::Stopped(_, _)) => Ok(Self { pid }),
            Ok(status) => Err(format!("Unexpected wait status after attach: {:?}", status)),
            Err(e) => Err(format!("waitpid failed: {}", e)),
        }
    }

    /// Check if the traced process is still alive.
    #[allow(dead_code)]
    pub fn is_running(&self) -> bool {
        unsafe { libc::kill(self.pid, 0) == 0 }
    }
}
