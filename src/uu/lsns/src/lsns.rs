// This file is part of the uutils util-linux package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use std::fs::DirEntry;

use clap::{crate_version, Command};
use uucore::{error::UResult, format_usage, help_about, help_usage};

const ABOUT: &str = help_about!("lsns.md");
const USAGE: &str = help_usage!("lsns.md");
const PATH_PROC: &str = "/proc";

const NSNAMES: [&str; 8] = ["cgroup", "ipc", "mnt", "net", "pid", "user", "uts", "time"];

enum NamespaceType {
    Cgroup = 0,
    Ipc = 1,
    Mnt = 2,
    Net = 3,
    Pid = 4,
    User = 5,
    Uts = 6,
    Time = 7,
}

// Struct to store process information
struct Process {
    // Process ID - unique identifier for this process
    pid: u32,
    // Parent's PID
    ppid: u32,
    // Thread Group ID - identifies the thread group leader
    tpid: u32,
    // Process state (R=running, S=sleeping, D=disk, Z=zombie, T=stopped, etc.)
    state: char,
    // User ID - the user that owns this process
    uid: u32,
    // Namespace inode IDs for each namespace type
    ns_ids: [u32; 8],
    // Parent namespace inode IDs (for hierarchical namespaces like pid, user)
    ns_pids: [u64; 8],
    // Owner namespace inode IDs (for user namespace)
    ns_oids: [u64; 8],
    // Network namespace ID - used by the network subsystem
    netnsid: i32,
}

struct Namespace {
    // Namespace ID - unique identifier for this namespace
    id: u32,
    // Namespace type
    ns_type: NamespaceType,
    // Number of processes in this namespace
    nprocs: u32,
    // Network namespace ID - used by the network subsystem
    netnsid: i32,
}

struct Lsns {
    processes: Vec<Process>,
    namespaces: Vec<Namespace>,
}

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let _matches = uu_app().try_get_matches_from(args)?;

    println!("This is lsns utility");
    read_processes(PATH_PROC)?;
    Ok(())
}

pub fn uu_app() -> Command {
    Command::new(uucore::util_name())
        .version(crate_version!())
        .about(ABOUT)
        .override_usage(format_usage(USAGE))
        .infer_long_args(true)
}

fn read_processes(path: &str) -> std::io::Result<()> {
    for entry in std::fs::read_dir(path)? {
        if entry.is_err() {
            continue;
        }

        let pid: u64 = match get_pid_from_entry(&entry.as_ref().unwrap()) {
            Some(p) => p,
            None => continue,
        };

        println!("PID: {}", pid);
    }
    Ok(())
}

/// Check if a directory entry in /proc represents a process.
/// If so, returns the PID, None otherwise
fn get_pid_from_entry(entry: &DirEntry) -> Option<u64> {
    let file_name = entry.file_name();
    let name = match file_name.to_str() {
        Some(s) => s,
        None => return None,
    };

    // Check if name starts with a digit (process directories are numeric PIDs)
    let is_digit = name
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false);

    if !is_digit {
        return None;
    }

    // Try to parse the name as a PID
    let pid = match name.parse::<u64>() {
        Ok(p) => p,
        Err(_) => return None,
    };

    Some(pid)
}
