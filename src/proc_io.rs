use std::fs::{read_dir, DirEntry, File};
use std::io::prelude::*;

use proc_structures::*;

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

    ProcessInformation::new_from_stat(&stat_fields)
}

/// Get information about the running processes from `/proc`
pub fn get_process_info() -> Vec<ProcessInformation> {
    let mut process_list = Vec::new();

    // read all directories in the proc pseudo-filesystem, with filters:
    // 1. ignore paths that threw errors
    // 2. filter paths that do not correspond to process IDs
    // 3. filter paths where we don't have the right permissions to open sensitive files
    let proc_dirs = read_dir("/proc/").unwrap()
        .filter_map(|dir_result| { dir_result.ok() })
        .filter(|dir_entry| {
            // must be a directory
            dir_entry.path().is_dir()
        })
        .filter(|dir_entry| {
            // must have a numerical (=pid) directory name
            dir_entry.file_name().to_str().unwrap().parse::<u64>().is_ok()
        })
        .filter(|dir_entry| {
            // must be able to open `/proc/[pid]/maps`
            File::open(dir_entry.path().join("maps")).is_ok()
        })
        .collect::<Vec<_>>();

    println!("Found {} processes", proc_dirs.len());

    for dir_entry in proc_dirs {
        let process_info = get_process_metadata(dir_entry);
        process_list.push(process_info);
    }

    process_list
}
