use std::fs::File;
use std::io::prelude::*;
use std::io;

use std::fmt;

use std::collections::HashMap;
use byteorder::{NativeEndian, ReadBytesExt};

/// Page size for many linux variants (at least Ubuntu..)
///
/// Could be changed to get it programatically in Rust (e.g. `getpagesize` in glibc).
pub const LINUX_PAGE_SIZE: usize = 4096;


bitflags! {
    /// Memory region permissions, as they are mapped by `mmap`
    ///
    /// if `SHARED` is not set, then the visibility of the mapping is `PRIVATE`
    pub struct MemoryPermissions: u8 {
        const READ = 0x01;
        const WRITE = 0x02;
        const EXECUTE = 0x04;
        const SHARED = 0x08;
    }
}


/// A `PageLocation` type indicating whether a page is in RAM or swapped out.
///
/// A `NONE` case was also added in case the page isn't used or there were access errors.
#[derive(Debug, PartialEq)]
enum PageLocation {
    /// Location in RAM, in page frames.
    RAM(usize),
    /// Location in SWAP, first index is swap type, second index is offset in that swap.
    SWAP(usize, usize),
    /// Invalid page/no page found.
    NONE,
}


/// Store information about physical page frames.
///
/// By default, only high-level information is present, such as:
/// - Whether the page is in RAM or in swap
/// - if the page is file-mapped or anonymous
/// - if the page table entry is soft-dirty
///   (this seems to be used mostly for tracing page accesses)
/// - the page frame number (PFN), if present
///
/// The swap type and offset are currently not stored, if the page is swapped out.
#[derive(Debug, PartialEq)]
pub struct PageFrame {
    page_location: PageLocation,
    is_file_page: bool,
    is_soft_dirty: bool,
}


/// A `PageFrameRegion` indicates a number of successive repeating `PageFrame` structs.
#[derive(Debug)]
struct PageFrameRegion {
    frame: PageFrame,
    len: usize,
}


struct PageFrameMap(HashMap<usize, PageFrameRegion>);


/// Describes a memory region as contained in `/proc/[pid]/maps`
#[derive(Debug)]
pub struct MemoryRegion {
    /// virtual memory region
    v_region_start: usize,
    v_region_end: usize,
    permissions: MemoryPermissions,

    /// mapped file location and offset in the file
    offset: usize,
    pathname: Option<String>,

    /// the corresponding physical regions that make up the virtual range
    physical_regions: Option<PageFrameMap>,
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


impl PageFrame {
    /// Construct a new `PageFrame` struct from its binary encoding.
    pub fn new(page_index: u64) -> Self {
        let ram_flag = page_index & (1 << 63) != 0;
        let swap_flag = page_index & (1 << 62) != 0;

        // apparently pages can be neither in RAM nor in SWAP, so the NONE case
        // was added. That probably means that page was allocated by never accessed.
        let page_location =
            if ram_flag {
                // Bits 0-54 indicate Page Frame Number (PFN)
                let pfn = (page_index & ((1 << 55) - 1)) as usize;
                PageLocation::RAM(pfn)
            } else if swap_flag {
                // Bits 0-4 indicate the Swap Type
                let swap_type = (page_index & 0b0001_1111) as usize;
                // Bite 5-54 indicate Swap Offset
                let swap_offset = ((page_index >> 5) & ((1 << 50) - 1)) as usize;
                PageLocation::SWAP(swap_type, swap_offset)
            } else {
                PageLocation::NONE
            };

        let is_file_page = page_index & (1 << 61) != 0;
        let is_soft_dirty = page_index & (55 << 1) != 0;

        PageFrame {
            page_location,
            is_file_page,
            is_soft_dirty,
        }
    }

    /// Determine if this `PageFrame` comes before the `other: PageFrame`
    /// This function is used to detect runs of identical page frames.
    pub fn is_previous_page(&self, other: &Self) -> bool {
        // the basic attributes have to be equal anyway
        if (self.is_file_page != other.is_file_page) ||
            (self.is_soft_dirty != other.is_soft_dirty) {
            return false;
        }

        // the locations must have the same type (RAM or SWAP) and the ordering must be right.
        match (&self.page_location, &other.page_location) {
            (PageLocation::RAM(pfn1), PageLocation::RAM(pfn2)) => ((*pfn1 + 1) == *pfn2),
            (PageLocation::SWAP(type1, offset1), PageLocation::SWAP(type2, offset2)) => {
                (*type1 == *type2) && ((*offset1 + 1) == *offset2)
            },
            _ => false,
        }
    }
}


impl fmt::Debug for PageFrameMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (key, value) in &self.0 {
            writeln!(f, "{:?} - {:?}", key, value)?;
        }

        Ok(())
    }
}


impl MemoryRegion {
    pub fn fill_physical_maps(&mut self, pagemap: &mut File) -> io::Result<()> {
        if self.pathname.is_some() {
            println!("Finding physical maps for {}", self.pathname.as_ref().unwrap());
        }

        let mut physical_map: PageFrameMap = PageFrameMap(HashMap::new());

        // start and end page numbers
        let page_index_start = self.v_region_start / LINUX_PAGE_SIZE;
        let page_index_end = self.v_region_end / LINUX_PAGE_SIZE;

        // page start and read length in bytes (one page has a 64bit entry)
        let page_start_bytes = page_index_start * 8;
        let read_length_bytes = (page_index_end - page_index_start + 1) * 8;

        // seek to the first page index of the memory region
        pagemap.seek(io::SeekFrom::Start(page_start_bytes as u64))?;

        // read the all page indices at once
        let mut byte_buf = vec![0u8; read_length_bytes];
        pagemap.read(&mut byte_buf)?;

        // convert bytes to u64
        let mut buf_rdr = io::Cursor::new(byte_buf);
        let mut u64_buf = vec![0u64; read_length_bytes / 8];
        buf_rdr.read_u64_into::<NativeEndian>(&mut u64_buf).unwrap();

        let mut page_frames = u64_buf.into_iter()
            .map(PageFrame::new)
            .zip(page_index_start..page_index_end);

        // check if the iterator is empty, and if so, terminate early
        let first_page = page_frames.next();
        if first_page.is_none() {
            self.physical_regions = None;
            return Ok(());
        }

        // unwrap the first page of the iterator and iterate through the rest
        let (mut last_frame, mut v_start) = first_page.unwrap();
        for (page_frame, v_page) in page_frames {
            // we combine sequences of identical frames or frames that follow another
            if (last_frame != page_frame) && (!last_frame.is_previous_page(&page_frame)) {
                let frame = PageFrameRegion { frame: last_frame, len: v_page - v_start };
                physical_map.0.insert(v_start,
                                      frame);
                v_start = v_page;
            }

            last_frame = page_frame;
        }

        // add the last open PageFrameRegion
        physical_map.0.insert(v_start,
                              PageFrameRegion{ frame: last_frame, len: page_index_end - v_start});

        // if the physical address map is empty, insert None. Else insert the map.
        self.physical_regions = Some(physical_map);

        Ok(())
    }

    /// Constructs a new `MemoryRegion` given components of the `/proc/[pid]/maps` lines
    pub fn new_from_map_fields(map_fields: &Vec<&str>) -> Self {
        let address = map_fields[0].split('-').collect::<Vec<_>>();
        let v_region_start = usize::from_str_radix(address[0], 16).unwrap();
        let v_region_end = usize::from_str_radix(address[1], 16).unwrap();

        // verify that the region is valid
        assert!(v_region_start < v_region_end,
                "Expect region start < end. (Have {} >= {})", v_region_start, v_region_end);

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
            v_region_start,
            v_region_end,
            offset,
            pathname,
            permissions,
            physical_regions: None,
        }
    }

    pub fn has_physical_mapping(&self) -> bool {
        return self.physical_regions.is_some();
    }
}
