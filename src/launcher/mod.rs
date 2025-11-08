use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

pub struct ProcessLauncher {
    pub pid: i32,
    paused: bool,
}

impl ProcessLauncher {
    pub fn launch(program: &str, args: &[String]) -> Result<Self, String> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        let child = cmd.spawn()
            .map_err(|e| format!("Failed to spawn process: {}", e))?;

        let pid = child.id() as i32;
        
        let _ = kill(Pid::from_raw(pid), Signal::SIGSTOP);
        thread::sleep(Duration::from_millis(50));

        Ok(Self { pid, paused: true })
    }
    
    pub fn resume(&mut self) -> Result<(), String> {
        if self.paused {
            kill(Pid::from_raw(self.pid), Signal::SIGCONT)
                .map_err(|e| format!("Failed to resume process: {}", e))?;
            self.paused = false;
        }
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        unsafe {
            let result = libc::kill(self.pid, 0);
            result == 0
        }
    }

    pub fn wait_for_exit(&self) {
        while self.is_running() {
            thread::sleep(Duration::from_millis(100));
        }
    }
}

