use std::fs::{read_dir, File};
use std::path::PathBuf;
use std::io::prelude::*;
use std::io;

use std::thread;
use std::time::Duration;
use std::sync::mpsc::{sync_channel, Receiver, TrySendError, TryRecvError};

use super::process::ProcessInformation;

/// Parse process metadata from the `/proc/[pid]/stat` file
///
/// Precondition: dir_entry must be a valid process information directory.
fn get_process_metadata(dir_path: PathBuf, load_mapping: bool) -> ProcessInformation {
    // parse the /proc/pid/stat file
    let stat_path = dir_path.join("stat");
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

    ProcessInformation::new_from_stat(&stat_fields, load_mapping)
}

pub fn get_pid_info(pid: u64) -> io::Result<ProcessInformation> {
    // check if the directory exists and we can access it
    let process_path = PathBuf::from(format!("/proc/{}", pid));
    let file_result = File::open(process_path.join("maps"));
    if let Err(err) = file_result {
        eprintln!("Could not access {:?}", process_path);
        eprintln!("{}", err.to_string());
        return Err(err);
    }

    let process_info = get_process_metadata(PathBuf::from(process_path),
                                            true);
    Ok(process_info)
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

    // get metadata info about the PIDs we collected
    for dir_entry in proc_dirs {
        let process_info = get_process_metadata(dir_entry.path(),
                                                false);
        process_list.push(process_info);
    }

    process_list
}


pub struct ProcScanner {
    proc_receiver: Receiver<Vec<ProcessInformation>>,
}


impl ProcScanner {
    pub fn new() -> Self {
        let (sync_sender, receiver) = sync_channel(1);

        // spawn the thread
        thread::spawn(move || {
            loop {
                let process_list = get_process_info();

                // if we have an error while sending, check the error type
                if let Err(send_error) = sync_sender.try_send(process_list) {

                    // if the queue is full, do nothing.
                    // at receiver disconnect, shut down thread.
                    match send_error {
                        TrySendError::Full(_) => {
                            println!("Queue is full!")
                        },
                        TrySendError::Disconnected(_) => {
                            println!("Terminating the process scanning thread.");
                            break;
                        }
                    };
                } else {
                    println!("Successfully enqueued new process info.");
                }

                // only scan the processes every 2 seconds
                // TODO: think about whether sleeping here is best (or after starting the scan?)
                thread::sleep(Duration::from_secs(2));
            }
        });

        ProcScanner {
            proc_receiver: receiver,
        }
    }

    pub fn process_info(&self) -> Option<Vec<ProcessInformation>> {
        // return the receiver process information, if any was returned
        // TODO: sender disconnect could happen when the process scan thread crashes, handle that
        self.proc_receiver.try_recv().ok()
    }
}