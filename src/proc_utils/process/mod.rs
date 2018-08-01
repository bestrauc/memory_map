pub mod memory;

use std::fmt;
use std::fs::File;
use std::io::prelude::*;
use self::memory::MemoryRegion;

/// Enum to express the process states from `/proc/[pid]/stat`
/// The comment next to the variant is the shortcut in the stat file.
#[derive(Debug)]
pub enum ProcessState {
    // R
    Running,
    // S
    Sleeping,
    // D
    Waiting,
    // Z
    Zombie,
    // T
    Stopped,
    // t
    Tracing,
    // X
    Dead,
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

/// The memory map of a process.
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
    pub fn new_memory_map(pid: usize, map_physical: bool) -> Self {
        let mut mem = Vec::new();

        // open the `/proc/[pid]/maps` file and parse the memory regions
        let maps_path = format!("/proc/{}/maps", pid);
        let mut maps_file = File::open(&maps_path)
            .expect(&format!("Could not open file '{}'", maps_path));

        let mut pagemap_option = {
            if !map_physical {
                None
            } else {
                let pagemap_path = format!("/proc/{}/pagemap", pid);
                let tmp_file = File::open(&pagemap_path);

                // log some error message if the file couldn't be opened
                if let Err(ref e) = tmp_file {
                    eprintln!("Could not open file '{}'", pagemap_path);
                    eprintln!("Error: {}", e.to_string());
                }

                tmp_file.ok()
            }
        };

        let mut maps_lines = String::new();
        maps_file.read_to_string(&mut maps_lines).unwrap();

        for line in maps_lines.lines() {
            let map_fields = line.split_whitespace().collect::<Vec<_>>();
            let mut region = MemoryRegion::new_from_map_fields(&map_fields);

            // if `map_physical` was set and we were able to read the pagemap, read it
            if pagemap_option.is_some() {
                let result = {
                    let pagemap_file = pagemap_option.as_mut().unwrap();
                    region.fill_physical_maps(pagemap_file)
                };

                // if an IO error occurred while reading the physical map, disable pagemap scanning
                if result.is_err() {
                    eprintln!("Error while scanning pagemap.");
                    pagemap_option = None;
                }
            }

            mem.push(region);
        }

        ProcessMemoryMap { regions: mem }
    }
}


#[derive(Debug)]
pub struct ProcessInformation {
    // Process metadata
    pid: usize,
    comm: String,
    state: ProcessState,

    // The mapped memory of the process.
    memory: ProcessMemoryMap,
}

impl ProcessInformation {
    /// Construct a new `ProcessInformation` from the parsed fields of `/proc/[pid]/stat`
    pub fn new_from_stat(stat_fields: &Vec<String>) -> Self {
        let pid = stat_fields[0].parse::<usize>().unwrap();

        ProcessInformation {
            pid,
            comm: stat_fields[1].clone(),
            state: ProcessState::new_from_code(stat_fields[2].chars().next().unwrap()),

            memory: ProcessMemoryMap::new_memory_map(pid, true),
        }
    }

    pub fn has_physical_map(&self) -> bool {
        self.memory.regions.last().is_some()
    }
}
