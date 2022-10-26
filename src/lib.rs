use cgroups_fs::{AutomanagedCgroup, Cgroup, CgroupName};
use std::{io, path::Path, time::Duration};
use sys_mount::UnmountFlags;
use tempfile::{self, TempDir};
use tokio::{process, time::timeout};

pub mod builder;
pub use builder::SandboxBuilder;

pub struct Sandbox {
    command: process::Command,

    root_dir: TempDir,
    _work_dir: TempDir,

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
    fn usage(&self) -> SandboxUsage {
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

    pub async fn run(&mut self) -> io::Result<SandboxOutput> {
        self.command.output().await.map(|output| SandboxOutput {
            output,
            usage: self.usage(),
        })
    }

    pub async fn run_with_timeout(
        &mut self,
        duration: Duration,
    ) -> Result<SandboxOutput, SandboxError> {
        let res = timeout(duration, self.command.output()).await;

        match res {
            Err(_) => Err(SandboxError::Elapsed),

            Ok(Ok(output)) => Ok(SandboxOutput {
                output,
                usage: self.usage(),
            }),
            Ok(Err(e)) => Err(SandboxError::IOError(e)),
        }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        sys_mount::unmount(self.root_dir.as_ref(), UnmountFlags::DETACH).unwrap();
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
