use cgroups_fs::{AutomanagedCgroup, CgroupName};
use std::path::{Path, PathBuf};
use tokio::process;
use uuid::Uuid;

use super::{CommandExt, Sandbox};

pub struct SandboxBuilder {
    memory_limit: Option<i64>,
    pids_limit: Option<i64>,

    command: String,
    args: Vec<String>,

    root_fs: PathBuf,
    upper_dir: PathBuf,
}

impl SandboxBuilder {
    pub fn new(command: &str, root_fs: impl AsRef<Path>, upper: impl AsRef<Path>) -> Self {
        SandboxBuilder {
            memory_limit: None,
            pids_limit: None,

            command: String::from(command),
            args: Vec::new(),

            root_fs: root_fs.as_ref().to_path_buf(),
            upper_dir: upper.as_ref().to_path_buf(),
        }
    }

    pub fn arg(mut self, arg: &str) -> Self {
        self.args.push(arg.to_string());
        self
    }

    pub fn args<S: ToString>(mut self, args: &[S]) -> Self {
        args.into_iter().for_each(|s| {
            self.args.push(s.to_string());
        });

        self
    }

    pub fn memory(mut self, memory: i64) -> Self {
        self.memory_limit = Some(memory);
        self
    }

    pub fn pids(mut self, pids: Option<i64>) -> Self {
        self.pids_limit = pids;
        self
    }

    pub fn build(self) -> Sandbox {
        // Build Command
        let mut command = process::Command::new(self.command);

        command.args(self.args);

        // Build Overlay FS
        let root_dir = tempfile::tempdir().unwrap();
        let work_dir = tempfile::tempdir().unwrap();

        let overlay = libmount::Overlay::writable(
            [self.root_fs.as_ref()].into_iter(),
            &self.upper_dir,
            work_dir.as_ref(),
            root_dir.as_ref(),
        );

        overlay.mount().unwrap();

        // Build Cgroup
        /* let cgroup_name = Uuid::new_v4().to_string();
        let hier = cgroups_rs::hierarchies::auto();
        let cg = cgroups_rs::cgroup_builder::CgroupBuilder::new(&cgroup_name);

        let cg = cg.pid().maximum_number_of_processes(self.pids_limit).done();

        let cg = if let Some(memory) = self.memory_limit {
            cg.memory().memory_hard_limit(memory).done()
        } else {
            cg
        };

        cg.build(hier); */

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
        let command = command
            .cgroup(&raw_name, &["memory", "cpuacct", "pids"])
            .chroot(&root_dir);

        Sandbox {
            // cgroup_name,
            command,

            root_dir,
            _work_dir: work_dir,

            memory,
            cpuacct,
            _pids: pids,
        }
    }
}
