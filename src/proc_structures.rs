use std::fs::{File};
use std::io::prelude::*;
use std::fmt;

/// Page size for many linux variants (at least Ubuntu..)
/// Find out hot to look it up programatically in Rust (e.g. `getpagesize` in glibc).
const LINUX_PAGE_SIZE: u64 = 4096;

/// Enum to express the process states from `/proc/[pid]/stat`
/// The comment next to the variant is the shortcut in the stat file.
#[derive(Debug)]
pub enum ProcessState {
    Running,    // R
    Sleeping,   // S
    Waiting,    // D
    Zombie,     // Z
    Stopped,    // T
    Tracing,    // t
    Dead,       // X
}

impl ProcessState {
    pub fn new_from_code(state_code: char) -> ProcessState {
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


bitflags! {
    /// Memory region permissions, as they are mapped by `mmap`
    /// if `SHARED` is not set, then the visibility of the mapping is `PRIVATE`
    pub struct MemoryPermissions: u8 {
        const READ = 0x01;
        const WRITE = 0x02;
        const EXECUTE = 0x04;
        const SHARED = 0x08;
    }
}

/// Describes a memory region as contained in `/proc/[pid]/maps`
#[derive(Debug)]
pub struct MemoryRegion {
    start: u64,
    end: u64,
    offset: u64,
    permissions: MemoryPermissions,
    pathname: Option<String>,
}

impl MemoryRegion {
    /// Constructs a new `MemoryRegion` given components of the `/proc/[pid]/maps` lines
    pub fn new_from_map_fields(map_fields: &Vec<&str>) -> Self {
        let address = map_fields[0].split('-').collect::<Vec<_>>();
        let start = u64::from_str_radix(address[0], 16).unwrap();
        let end = u64::from_str_radix(address[1], 16).unwrap();

        // verify that the region is valid
        assert!(start < end, "Expect region start < end. (Have {} >= {})", start, end);

        let offset = u64::from_str_radix(map_fields[2], 16).unwrap();
        let pathname = {
            if map_fields.len() < 6 {
                None
            } else {
                Some(map_fields[5].to_string())
            }
        };

        let perm_str = map_fields[1];
        // parse the permission string, which must have 4 characters
        assert_eq!(perm_str.len(), 4,
                   "Malformed permission field '{}', expected {} characters", perm_str, 4);

        let mut permissions = MemoryPermissions::empty();
        let perm_str = perm_str.as_bytes();
        if perm_str[0] == b'r' { permissions = permissions | MemoryPermissions::READ };
        if perm_str[1] == b'w' { permissions = permissions | MemoryPermissions::WRITE };
        if perm_str[2] == b'x' { permissions = permissions | MemoryPermissions::EXECUTE };
        if perm_str[3] == b's' { permissions = permissions | MemoryPermissions::SHARED };

        MemoryRegion {
            start,
            end,
            offset,
            pathname,
            permissions,
        }
    }
}


/// The memory map of a process.
//#[derive(Debug)]
pub struct ProcessMemoryMap {
    regions: Vec<MemoryRegion>,
}

impl fmt::Debug for ProcessMemoryMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for region in &self.regions {
            writeln!(f, "{:?}", region)?;
        }

        Ok(())
    }
}

impl ProcessMemoryMap {
    /// Retrieve the virtual page boundaries from `/proc/[pid]/maps`
    pub fn read_page_map(pid: u64) -> ProcessMemoryMap {
        let mut mem = Vec::new();

        // open the `/proc/[pid]/maps` file and parse the memory regions
        let maps_path = format!("/proc/{}/maps", pid);
        let mut maps_file = File::open(&maps_path)
            .expect(&format!("Could not open file '{}'", maps_path));

        let mut maps_lines = String::new();
        maps_file.read_to_string(&mut maps_lines).unwrap();

        for line in maps_lines.lines() {
            let map_fields = line.split_whitespace().collect::<Vec<_>>();
            let region = MemoryRegion::new_from_map_fields(&map_fields);

            mem.push(region);
        }

        ProcessMemoryMap { regions: mem }
    }
}


#[derive(Debug)]
pub struct ProcessInformation {
    // Process metadata
    pid: u64,
    comm: String,
    state: ProcessState,

    // The mapped memory of the process.
    memory: ProcessMemoryMap,
}

impl ProcessInformation {
    /// Construct a new `ProcessInformation` from the parsed fields of `/proc/[pid]/stat`
    pub fn new_from_stat(stat_fields: &Vec<String>) -> Self {
        let pid = stat_fields[0].parse::<u64>().unwrap();

        ProcessInformation {
            pid,
            comm: stat_fields[1].clone(),
            state: ProcessState::new_from_code(stat_fields[2].chars().next().unwrap()),

            memory: ProcessMemoryMap::read_page_map(pid),
        }
    }
}
