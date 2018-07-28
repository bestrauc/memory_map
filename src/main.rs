extern crate memory_map;
use memory_map::proc_io;

use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    let process_list = proc_io::get_process_info();
    for process in process_list {
//        println!("{:?}", process);
    }
}
