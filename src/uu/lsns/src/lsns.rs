// This file is part of the uutils util-linux package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use std::fs::DirEntry;

use clap::{crate_version, Command};
use std::fs;
use std::os::linux::fs::MetadataExt;
use uucore::{error::UResult, format_usage, help_about, help_usage};

const ABOUT: &str = help_about!("lsns.md");
const USAGE: &str = help_usage!("lsns.md");
const PATH_PROC: &str = "/proc";
const LSNS_NETNS_UNUSABLE: i32 = -2;
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
    ns_ids: [u64; 8],
    // Parent namespace inode IDs (for hierarchical namespaces like pid, user)
    ns_pids: [u64; 8],
    // Owner namespace inode IDs (for user namespace)
    ns_oids: [u64; 8],
    // Network namespace ID - used by the network subsystem
    netnsid: i32,
    ns_siblings: [Vec<String>; 8],
}

impl Process {
    /// Creates a new instance with the given PID
    pub fn new() -> Self {
        Self {
            pid: 0,
            ppid: 0,
            tpid: 0,
            state: ' ',
            uid: 0,
            ns_ids: [0; 8],
            ns_pids: [0; 8],
            ns_oids: [0; 8],
            netnsid: 0,
            ns_siblings: Default::default(),
        }
    }
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
    let mut lsns = Lsns {
        processes: Vec::new(),
        namespaces: Vec::new(),
    };
    read_processes(PATH_PROC, &mut lsns)?;
    read_namespaces(&mut lsns);
    Ok(())
}

pub fn uu_app() -> Command {
    Command::new(uucore::util_name())
        .version(crate_version!())
        .about(ABOUT)
        .override_usage(format_usage(USAGE))
        .infer_long_args(true)
}

fn read_processes(path: &str, lsns: &mut Lsns) -> std::io::Result<()> {
    for entry in std::fs::read_dir(path)? {
        let _entry: DirEntry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let pid: u64 = match get_pid_from_entry(&_entry) {
            Some(p) => p,
            None => continue,
        };

        let process = match read_process(&_entry, pid as i32) {
            Some(p) => p,
            None => continue,
        };
        lsns.processes.push(process);

        // let uid = match get_uid_from_entry(&_entry) {
        //     Some(u) => u,
        //     None => continue,
        // };

        // let stat = match get_process_stat(&_entry) {
        //     Some(s) => s,
        //     None => continue,
        // };

        // let (pid, state, ppid) = match parse_process_stat(&stat) {
        //     Some(s) => s,
        //     None => continue,
        // };

        // println!("PID:UID:PPID:STATE {}:{}:{}:{}", pid, uid, ppid, state);
        // println!("====================================");
    }
    Ok(())
}

/// Parse /proc/[pid]/stat content to extract PID, state, and PPID
///
/// Format: PID (COMMAND) STATE PPID ...
/// The command name can contain spaces and parentheses
fn parse_process_stat(stat: &str) -> Option<(u32, char, u32)> {
    // Find the last ')' - handles command names with parentheses
    let rparen_pos = stat.rfind(')')?;

    // Find the first '(' - marks start of command name
    let lparen_pos = stat.find('(')?;

    // Validate positions
    if lparen_pos >= rparen_pos {
        return None;
    }

    // Extract PID (everything before the '(')
    let pid_str = stat[..lparen_pos].trim();
    let pid: u32 = pid_str.parse().ok()?;

    // Extract state and PPID (everything after the ')')
    // Format after ')': " STATE PPID ..."
    let after_paren = &stat[rparen_pos + 1..];
    let mut parts = after_paren.split_whitespace();

    // Get state (first field after ')')
    let state_str = parts.next()?;
    let state: char = state_str.chars().next()?;

    // Get PPID (second field after ')')
    let ppid_str = parts.next()?;
    let ppid: u32 = ppid_str.parse().ok()?;

    Some((pid, state, ppid))
}

fn get_uid_from_entry(entry: &DirEntry) -> Option<u32> {
    let f = entry.metadata().ok()?;
    let uid = f.st_uid();
    Some(uid)
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

/// Get namespace inode numbers for a process
///
/// Reads /proc/[pid]/ns/[nsname] to get:
/// - ino: The namespace's own inode
/// - pino: Parent namespace inode (for hierarchical namespaces)
/// - oino: Owner user namespace inode
fn get_ns_inos(pid: u32, nsname: &str) -> Option<(u64, u64, u64)> {
    let ns_path = format!("/proc/{}/ns/{}", pid, nsname);

    // Get the namespace inode by stat'ing the namespace file
    let metadata = fs::metadata(&ns_path).ok()?;
    let ino = metadata.st_ino();

    // For now, we don't get parent and owner inodes
    // (requires ioctl NS_GET_PARENT and NS_GET_USERNS, which need unsafe code)
    let pino = 0;
    let oino = 0;

    Some((ino, pino, oino))
}

/// Integration into read_process
fn read_process(entry: &DirEntry, pid: i32) -> Option<Process> {
    let mut process = Process::new();
    process.pid = pid as u32;
    process.netnsid = LSNS_NETNS_UNUSABLE;

    process.uid = get_uid_from_entry(entry)?;

    // Read and parse /proc/[pid]/stat
    let stat_path = format!("/proc/{}/stat", pid);

    let stat_content = match fs::read_to_string(&stat_path) {
        Ok(s) => s,
        Err(_) => return None,
    };

    let (pid, state, ppid) = parse_process_stat(&stat_content)?;

    process.pid = pid;
    process.state = state;
    process.ppid = ppid;

    // Get namespace inodes for all namespace types
    // let ns_inodes = get_all_ns_inos(pid)?;

    for (i, nsname) in NSNAMES.iter().enumerate() {
        let (ino, pino, oino) = match get_ns_inos(pid, nsname) {
            Some((i, p, o)) => (i, p, o),
            None => continue,
        };

        process.ns_ids[i] = ino;
        process.ns_pids[i] = pino;
        process.ns_oids[i] = oino;
    }

    // TODO: Get network namespace ID via netlink
    // if process.ns_ids[3] != 0 { // LSNS_TYPE_NET = 3
    //     process.netnsid = get_netnsid_for_process(pid, process.ns_ids[3])?;
    // }
    // lsns.processes.push(process);

    // TODO: Read opened namespaces. Check read_opened_namespaces function in lsns.c
    Some(process)
}

fn read_namespaces(lsns: &mut Lsns) {
    read_assigned_namespaces(lsns);
}

/// Read and organize namespaces from the processes we've collected
///
/// This is Phase 2 of data collection:
/// - Phase 1: read_processes() collected all process information
/// - Phase 2: This function groups processes by their namespaces
///
/// What it does:
/// 1. Iterates through all processes
/// 2. For each namespace a process belongs to, creates a Namespace struct (if new)
/// 3. Links processes to their namespaces
/// 4. Counts processes per namespace
fn read_assigned_namespaces(lsns: &mut Lsns) {
    // We'll use a HashMap to track namespaces by inode for quick lookup
    // Key: namespace inode, Value: index in lsns.namespaces vector
    let mut namespace_map: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();

    // Iterate through all processes we collected
    for proc_idx in 0..lsns.processes.len() {
        let process = &lsns.processes[proc_idx];

        // For each of the 8 namespace types (mnt, net, pid, uts, ipc, user, cgroup, time)
        for ns_type_idx in 0..8 {
            // Get the namespace inode for this process and namespace type
            let ns_inode = process.ns_ids[ns_type_idx];

            // Skip if this process doesn't have this namespace type
            // (inode = 0 means not present)
            if ns_inode == 0 {
                continue;
            }

            // Check if we've already created a Namespace struct for this inode
            let ns_idx = if let Some(&idx) = namespace_map.get(&ns_inode) {
                // Namespace already exists - use existing index
                idx
            } else {
                // This is a new namespace - create it

                // For network namespaces, use the network namespace ID we queried earlier
                // For other types, mark as unusable (-2)
                let netnsid = if ns_type_idx == 3 {
                    // Index 3 = Net namespace
                    process.netnsid
                } else {
                    LSNS_NETNS_UNUSABLE
                };

                // Create the new namespace
                let namespace = Namespace {
                    id: ns_inode as u32, // Cast to match your Namespace.id type
                    ns_type: NamespaceType::from_index(ns_type_idx),
                    nprocs: 0, // Will increment as we add processes
                    netnsid,
                };

                // Add to our namespace list
                let idx = lsns.namespaces.len();
                lsns.namespaces.push(namespace);

                // Remember this namespace's index for future lookups
                namespace_map.insert(ns_inode, idx);

                idx
            };

            // Now increment the process count for this namespace
            lsns.namespaces[ns_idx].nprocs += 1;
        }
    }
}

/// Helper to convert namespace type index to enum
impl NamespaceType {
    fn from_index(idx: usize) -> Self {
        match idx {
            0 => NamespaceType::Cgroup,
            1 => NamespaceType::Ipc,
            2 => NamespaceType::Mnt,
            3 => NamespaceType::Net,
            4 => NamespaceType::Pid,
            5 => NamespaceType::User,
            6 => NamespaceType::Uts,
            7 => NamespaceType::Time,
            _ => panic!("Invalid namespace type index: {}", idx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_stat() {
        let stat = "1234 (bash) S 1200 1234 1234 34816 1500";
        let result = parse_process_stat(stat);
        assert_eq!(result, Some((1234, 'S', 1200)));
    }

    #[test]
    fn test_parse_stat_with_parens_in_name() {
        let stat = "5678 (my app (v2)) R 1 5678 5678";
        let result = parse_process_stat(stat);
        assert_eq!(result, Some((5678, 'R', 1)));
    }

    #[test]
    fn test_parse_stat_with_spaces_in_name() {
        let stat = "9999 (Google Chrome) S 1234 9999 9999";
        let result = parse_process_stat(stat);
        assert_eq!(result, Some((9999, 'S', 1234)));
    }

    #[test]
    fn test_parse_invalid_stat() {
        assert_eq!(parse_process_stat("invalid"), None);
        assert_eq!(parse_process_stat("1234 bash S 1200"), None); // Missing parens
        assert_eq!(parse_process_stat(""), None);
    }
}
