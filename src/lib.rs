use builder::SandboxBuilder;
use cgroups_fs::{AutomanagedCgroup, Cgroup, CgroupName};
use std::{
    ffi::OsStr,
    io,
    path::Path,
    process::{ExitStatus, Output},
    time::Duration,
};
use sys_mount::UnmountFlags;
use tempfile::{self, TempDir};
use tokio::{
    process::{self, Command},
    time::timeout,
};

pub mod builder;

pub struct Sandbox {
    command: process::Command,

    _overlay_dir: Option<(TempDir, TempDir)>,

    time_limit: Option<Duration>,
    memory: AutomanagedCgroup,
    cpuacct: AutomanagedCgroup,
    _pids: AutomanagedCgroup,
}

#[derive(Debug)]
pub struct SandboxUsage {
    pub memory: u64,
    pub time: Duration,
}

#[derive(Debug)]
pub struct SandboxOutput {
    pub output: std::process::Output,
    pub usage: SandboxUsage,
}

#[derive(Debug)]
pub enum SandboxError {
    Elapsed,
    IOError(io::Error),
}

impl Sandbox {
    pub fn builder(command: impl AsRef<OsStr>) -> SandboxBuilder {
        SandboxBuilder {
            time_limit: None,
            memory_limit: None,
            pids_limit: None,

            command: Command::new(command),

            overlay: None,
        }
    }

    pub fn usage(&self) -> SandboxUsage {
        let memory = self
            .memory
            .get_value::<u64>("memory.max_usage_in_bytes")
            .unwrap();
        let time = self.cpuacct.get_value::<u64>("cpuacct.usage").unwrap();

        SandboxUsage {
            memory,
            time: Duration::from_nanos(time),
        }
    }

    pub async fn status(&mut self) -> Result<ExitStatus, SandboxError> {
        if let Some(duration) = self.time_limit {
            timeout(duration, self.command.status()).await.map_or_else(
                |_| Err(SandboxError::Elapsed),
                |r| r.map_err(|e| SandboxError::IOError(e)),
            )
        } else {
            self.command
                .status()
                .await
                .map_err(|e| SandboxError::IOError(e))
        }
    }

    pub async fn output(&mut self) -> Result<Output, SandboxError> {
        if let Some(duration) = self.time_limit {
            timeout(duration, self.command.output()).await.map_or_else(
                |_| Err(SandboxError::Elapsed),
                |r| r.map_err(|e| SandboxError::IOError(e)),
            )
        } else {
            self.command
                .output()
                .await
                .map_err(|e| SandboxError::IOError(e))
        }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if let Some((overlay_root, _)) = &self._overlay_dir {
            sys_mount::unmount(overlay_root.path(), UnmountFlags::DETACH).unwrap();
        }
    }
}

trait CommandExt {
    fn chroot(self, path: impl AsRef<Path>) -> Self;
    fn cgroup(self, cgroup_name: impl AsRef<Path>, sub_systems: &[&str]) -> Self;
}

impl CommandExt for process::Command {
    fn chroot(mut self, path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();

        unsafe {
            self.pre_exec(move || {
                std::os::unix::fs::chroot(&path)?;
                std::env::set_current_dir("/")?;

                Ok(())
            });
        }

        self
    }

    fn cgroup(mut self, cgroup_name: impl AsRef<Path>, sub_systems: &[&str]) -> Self {
        unsafe {
            let cgroup_name = CgroupName::new(cgroup_name);
            let sub_systems: Vec<_> = sub_systems.into_iter().map(|s| s.to_string()).collect();

            self.pre_exec(move || {
                let pid = std::process::id() as i32;

                sub_systems.iter().for_each(|sub| {
                    let cgroup = Cgroup::new(&cgroup_name, sub);
                    cgroup.add_task(nix::unistd::Pid::from_raw(pid)).unwrap();
                });

                Ok(())
            });

            self
        }
    }
}
