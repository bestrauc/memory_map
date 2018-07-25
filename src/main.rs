use std::fs;
use std::fs::{DirEntry, File};
use std::io::prelude::*;

/// Enum to express the process states from /proc/[pid]/stat
/// The comment next to the variant is the shortcut in the stat file.
#[derive(Debug)]
enum ProcessState {
    Running,    // R
    Sleeping,   // S
    Waiting,    // D
    Zombie,     // Z
    Stopped,    // T
    Tracing,    // t
    Dead,       // X
}


impl ProcessState {
    fn new_from_code(state_code: char) -> ProcessState {
        match state_code {
            'R' => ProcessState::Running,
            'S' => ProcessState::Sleeping,
            'D' => ProcessState::Waiting,
            'Z' => ProcessState::Zombie,
            'T' => ProcessState::Stopped,
            't' => ProcessState::Tracing,
            'X' => ProcessState::Dead,
            _ => panic!("Invalid state code '{}' encountered", state_code)
        }
    }
}

/// Describes a memory region as contained in `/proc/[pid]/maps`
#[derive(Debug)]
struct MemoryRegion {
    start: u64,
    end: u64,
    offset: u64,
}

impl MemoryRegion {
    /// Constructs a new `MemoryRegion` given address and offset as integers.
    fn new(start: u64, end: u64, offset: u64) -> Self {
        MemoryRegion {start, end, offset}
    }

    /// Constructs a new `MemoryRegion` given address and offset as hex strings.
    fn new_from_hex(start: &str, end: &str, offset: &str) -> Self {
        let start = u64::from_str_radix(start, 16).unwrap();
        let end = u64::from_str_radix(end, 16).unwrap();
        let offset = u64::from_str_radix(offset, 16).unwrap();

        Self::new(start, end, offset)
    }
}

/// The memory map of a process.
#[derive(Debug)]
struct ProcessMemoryMap {
    regions: Vec<MemoryRegion>,
}

impl ProcessMemoryMap {
    /// Parse the `/proc/[pid]/maps` for memory maps
    fn new(pid: u64) -> Self {
        let mem = vec![MemoryRegion::new(0,0,0)];
        ProcessMemoryMap {regions: mem}
    }
}


#[derive(Debug)]
struct ProcessInformation {
    // Process metadata
    pid: u64,
    comm: String,
    state: ProcessState,

    // The mapped memory of the process.
    memory: ProcessMemoryMap,
}

/// Parse process metadata from the `/proc/[pid]/stat` file
///
/// Precondition: dir_entry must be a valid process information directory.
fn get_process_metadata(dir_entry: DirEntry) -> ProcessInformation {
    // parse the /proc/pid/stat file
    let stat_path = dir_entry.path().join("stat");
    let stat_string = stat_path.to_str().unwrap().to_string();
    let mut stat_file = File::open(stat_path)
        .expect(&format!("Error: there should be a file '{}'", stat_string));

    let mut proc_metadata = String::new();
    stat_file.read_to_string(&mut proc_metadata)
        .expect(&format!("Error: could not read from file '{}'", stat_string));

    let mut stat_fields = proc_metadata.split_whitespace()
        .map(String::from)
        .collect::<Vec<_>>();

    // join the program name field, since it may contain whitespace
    let mut name_end = 1;
    while name_end < stat_fields.len() && !stat_fields[name_end].ends_with(')') {
        name_end += 1;
    }

    stat_fields[1] = stat_fields[1..(name_end+1)].join(" ");
    stat_fields.drain(2..(name_end+1));

    // split off brackets around the program name (abc) -> abc
    stat_fields[1] = stat_fields[1][1..stat_fields[1].len()-1].to_string();

    // should have exactly 52 metadata fields about the process
    assert_eq!(stat_fields.len(), 52,
               "Expected {} metadata fields, found {}", 52, stat_fields.len());

    let pid = stat_fields[0].parse::<u64>().unwrap();

    ProcessInformation {
        pid,
        comm: stat_fields[1].clone(),
        state: ProcessState::new_from_code(stat_fields[2].chars().next().unwrap()),

        memory: ProcessMemoryMap::new(pid)
    }
}

/// Get information about the running processes from `/proc`
fn get_process_list() -> Vec<ProcessInformation> {
    let mut process_list = Vec::new();

    // read all directories in the proc pseudo-filesystem
    // first ignore paths that threw errors and then
    // filter paths that do not correspond to process IDs
    let proc_dirs = fs::read_dir("/proc/").unwrap()
        .filter_map(|dir_result| { dir_result.ok() })
        .filter(|dir_entry| {
            dir_entry.path().is_dir() &&
                dir_entry.file_name().to_str().unwrap().parse::<u64>().is_ok()
        })
        .collect::<Vec<_>>();

    println!("Found {} processes", proc_dirs.len());

    for dir_entry in proc_dirs {
        let process_info = get_process_metadata(dir_entry);
        process_list.push(process_info);
    }

    process_list
}

/// Retrieve the virtual page boundaries from `/proc/[pid]/maps`
fn virtual_page_map(process: ProcessState) {

}

fn main() {
    let process_list = get_process_list();
    for process in process_list {
        println!("{:?}", process);
    }
}
