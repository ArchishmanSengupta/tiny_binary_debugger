use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

pub struct ProcessLauncher {
    pub pid: i32,
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
        
        thread::sleep(Duration::from_millis(100));

        Ok(Self { pid })
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

