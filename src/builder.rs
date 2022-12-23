use super::{CommandExt, Sandbox};
use cgroups_fs::{AutomanagedCgroup, CgroupName};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::process::Command;
use uuid::Uuid;

pub struct SandboxBuilder {
    pub(crate) time_limit: Option<Duration>,
    pub(crate) memory_limit: Option<i64>,
    pub(crate) pids_limit: Option<i64>,

    pub(crate) command: Command,

    pub(crate) overlay: Option<(PathBuf, PathBuf)>,
}

impl SandboxBuilder {
    pub fn memory(mut self, memory: i64) -> Self {
        self.memory_limit = Some(memory);
        self
    }

    pub fn time(mut self, duration: Duration) -> Self {
        self.time_limit = Some(duration);
        self
    }

    pub fn overlay(mut self, overlay: (impl AsRef<Path>, impl AsRef<Path>)) -> Self {
        self.overlay = Some((overlay.0.as_ref().to_owned(), overlay.1.as_ref().to_owned()));
        self
    }

    pub fn pids(mut self, pids: Option<i64>) -> Self {
        self.pids_limit = pids;
        self
    }

    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.command.arg(arg);
        self
    }

    pub fn args(mut self, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Self {
        self.command.args(args);
        self
    }

    pub fn stdin(mut self, cfg: impl Into<Stdio>) -> Self {
        self.command.stdin(cfg);
        self
    }

    pub fn stdout(mut self, cfg: impl Into<Stdio>) -> Self {
        self.command.stdout(cfg);
        self
    }

    pub fn stderr(mut self, cfg: impl Into<Stdio>) -> Self {
        self.command.stderr(cfg);
        self
    }

    pub fn build(self) -> Sandbox {
        // Build Overlay FS
        let overlay_dir = if let Some((lower_dir, upper_dir)) = self.overlay {
            let target_dir = tempfile::tempdir().unwrap();
            let work_dir = tempfile::tempdir().unwrap();

            let overlay = libmount::Overlay::writable(
                [lower_dir.as_path()].into_iter(),
                &upper_dir,
                work_dir.as_ref(),
                target_dir.as_ref(),
            );

            overlay.mount().unwrap();

            Some((target_dir, work_dir))
        } else {
            None
        };

        // Build Cgroup
        let raw_name = Uuid::new_v4().to_string();
        let cgroup_name = CgroupName::new(&raw_name);
        let memory = AutomanagedCgroup::init(&cgroup_name, "memory").unwrap();
        if let Some(m) = self.memory_limit {
            memory.set_value("memory.limit_in_bytes", m).unwrap();
            memory.set_value("memory.memsw.limit_in_bytes", m).unwrap();
        }

        let cpuacct = AutomanagedCgroup::init(&cgroup_name, "cpuacct").unwrap();

        let pids = AutomanagedCgroup::init(&cgroup_name, "pids").unwrap();
        if let Some(p) = self.pids_limit {
            pids.set_value("pids.max", p).unwrap();
        }

        // Apply Overlay and Cgroup
        let mut command = self
            .command
            .cgroup(&raw_name, &["memory", "cpuacct", "pids"]);
        if let Some((overlay_root, _)) = &overlay_dir {
            command = command.chroot(overlay_root);
        }

        Sandbox {
            // cgroup_name,
            command,

            _overlay_dir: overlay_dir,

            time_limit: self.time_limit,
            memory,
            cpuacct,
            _pids: pids,
        }
    }
}
