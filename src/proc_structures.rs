use std::fs::File;
use std::io::prelude::*;
use std::io;
use std::fmt;

use std::collections::HashMap;
use byteorder::{NativeEndian, ReadBytesExt};

/// Page size for many linux variants (at least Ubuntu..)
/// Find out hot to look it up programatically in Rust (e.g. `getpagesize` in glibc).
pub const LINUX_PAGE_SIZE: usize = 4096;

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

impl MemoryPermissions {
    pub fn new_from_str(perm_str: &str) -> Self {
        let mut permissions = MemoryPermissions::empty();
        let perm_str = perm_str.as_bytes();
        if perm_str[0] == b'r' { permissions = permissions | MemoryPermissions::READ };
        if perm_str[1] == b'w' { permissions = permissions | MemoryPermissions::WRITE };
        if perm_str[2] == b'x' { permissions = permissions | MemoryPermissions::EXECUTE };
        if perm_str[3] == b's' { permissions = permissions | MemoryPermissions::SHARED };

        permissions
    }
}

#[derive(Debug)]
pub struct MemoryRange(usize, usize);

/// Describes a memory region as contained in `/proc/[pid]/maps`
#[derive(Debug)]
pub struct MemoryRegion {
    // virtual memory region
    virtual_pages: MemoryRange,
    permissions: MemoryPermissions,

    // mapped file location and offset in the file
    offset: usize,
    pathname: Option<String>,

    // the corresponding physical regions that make up the virtual range
    physical_regions: Option<HashMap<usize, MemoryRange>>,
}

impl MemoryRegion {
    pub fn fill_physical_maps(&mut self, pagemap: &mut File) -> io::Result<()> {
        if self.pathname.is_some() {
            println!("Finding physical maps for {}", self.pathname.as_ref().unwrap());
        }

        let mut physical_map: HashMap<usize, MemoryRange> = HashMap::new();

        // start and end page numbers
        let page_start = (self.virtual_pages.0 / LINUX_PAGE_SIZE);
        let page_end = (self.virtual_pages.1 / LINUX_PAGE_SIZE);

        // page start and read length in bytes (one page has a 64bit entry)
        let page_start_bytes = page_start * 8;
        let read_length_bytes = (page_end - page_start + 1) * 8;

        // seek to the first page index of the memory region
        pagemap.seek(io::SeekFrom::Start(page_start_bytes as u64))?;

        // read the all page indices at once
        let mut byte_buf = vec![0u8; read_length_bytes];
        pagemap.read(&mut byte_buf)?;

        // convert bytes to u64
        let mut buf_rdr = io::Cursor::new(byte_buf);
        let mut u64_buf = vec![0u64; read_length_bytes / 8];
        buf_rdr.read_u64_into::<NativeEndian>(&mut u64_buf).unwrap();

//        println!("{} -- {}", self.virtual_pages.0, self.virtual_pages.1);
//        println!("{} -- {}", page_start, page_end);
//        for (a,b) in u64_buf.iter().zip(page_start..page_end) {
//            let in_ram = (a & (1 << 63)) != 0;
//            let page_frame_number = (a & ((1 << 55)-1)) as usize;
//            println!("{} : {} - {}", in_ram, page_frame_number, b);
//        }
//
//        return Ok(());

        // associate physical pages with their virtual addresses
        // and filter physical pages which are not in RAM
        // and map pages in RAM to their physical addresses
        let ram_pages = u64_buf
            .iter()
            .zip(page_start..page_end)
            .filter(|(page_val, _)| {
                // the last bit is set if page is in RAM
                (*page_val & (1 << 63)) != 0
            })
            .map(|(page_val, v_page)| {
                // only keep the bottom 55 bits
                ((*page_val & ((1 << 55)-1)) as usize, v_page)
            });

        // iterate over the values and find consecutive mappings to store in our map
        let mut physical_address: Option<MemoryRange> = None;
        let mut v_start = 0;
        let mut last_page_frame_number = 0;
        for (page_frame_number, v_page) in ram_pages {
            // start new address range if none exists yet
            if physical_address.is_none() {
                physical_address = Some(MemoryRange(page_frame_number, page_frame_number));
                v_start = v_page;
            } else {
                // extend existing range or start new one
                if page_frame_number == last_page_frame_number+1 {
                    let phy_adr = physical_address.as_mut().unwrap();
                    phy_adr.1 = page_frame_number;
                    assert!(phy_adr.0 < phy_adr.1);
                } else {
                    physical_map.insert(v_start, physical_address.unwrap());
                    physical_address = Some(MemoryRange(page_frame_number, page_frame_number));
                    v_start = v_page;
                }
            }

            last_page_frame_number = page_frame_number;
        }

        // insert the last physical memory region, if any was found
        if let Some(physical_mem_range) = physical_address {
            physical_map.insert(v_start, physical_mem_range);
        }

        // if the physical address map is empty, insert None. Else insert the map.
        self.physical_regions = if physical_map.is_empty() { None } else { Some(physical_map) };

        Ok(())
    }

    /// Constructs a new `MemoryRegion` given components of the `/proc/[pid]/maps` lines
    pub fn new_from_map_fields(map_fields: &Vec<&str>) -> Self {
        let address = map_fields[0].split('-').collect::<Vec<_>>();
        let start = usize::from_str_radix(address[0], 16).unwrap();
        let end = usize::from_str_radix(address[1], 16).unwrap();
        let virtual_pages = MemoryRange(start, end);

        // verify that the region is valid
        assert!(start < end, "Expect region start < end. (Have {} >= {})", start, end);

        let offset = usize::from_str_radix(map_fields[2], 16).unwrap();
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

        let permissions = MemoryPermissions::new_from_str(perm_str);

        MemoryRegion {
            virtual_pages,
            offset,
            pathname,
            permissions,
            physical_regions: None,
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
    pub fn read_virtual_map(pid: usize, map_physical: bool) -> Self {
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
//                    eprintln!("Could not open file '{}'", pagemap_path);
//                    eprintln!("Error: {}", e.to_string());
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

            memory: ProcessMemoryMap::read_virtual_map(pid, true),
        }
    }

    pub fn has_physical_map(&self) -> bool {
        self.memory.regions.last().is_some()
    }
}
