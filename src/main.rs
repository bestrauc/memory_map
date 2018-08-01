extern crate memory_map;
use memory_map::proc_utils;

use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    let target_pid = args[1].parse::<u64>().unwrap();

//    let process_list = proc_io::get_process_info();
    let process = proc_utils::io::get_pid_info(target_pid);
    println!("{:?}", process);

//    for process in process_list { println!("{:?}", process); }
}
