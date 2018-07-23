use std::fs;

#[derive(Debug)]
struct ProcessInformation {
    pid: u32,
}

/// Get information about the running processes from /proc
fn get_process_list() -> Vec<ProcessInformation> {
    let mut process_list = Vec::new();

    // read all directories in the proc pseudo-filesystem
    // first ignore paths that threw errors and then
    // filter paths that do not correspond to process IDs
    let proc_dirs = fs::read_dir("/proc/").unwrap()
        .filter_map(|dir_result| { dir_result.ok() })
        .filter(|dir_entry| {
            dir_entry.path().is_dir() &&
                dir_entry.file_name().to_str().unwrap().parse::<u32>().is_ok()
        })
        .collect::<Vec<_>>();

    println!("Found {} processes", proc_dirs.len());

    for dir_entry in proc_dirs {
        let dir_name = dir_entry.file_name();
        let dir_pid = dir_name.to_str().unwrap().parse::<u32>().unwrap();

        println!("Name: {}", dir_name.to_str().unwrap());
        process_list.push(ProcessInformation { pid: dir_pid} );
    }

    process_list
}

fn main() {
    let process_list = get_process_list();
    println!("Hello, world!");
    println!("{:?}", process_list);
}
