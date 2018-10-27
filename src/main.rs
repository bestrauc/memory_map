extern crate memory_map;
extern crate gio;
extern crate gtk;
extern crate glib;

use memory_map::proc_utils::io::ProcScanner;

use gio::prelude::*;
use gtk::prelude::*;

use std::env::args;


pub fn update_process_view(process_store: &gtk::ListStore, process_viewer: &ProcScanner) {
    // if no new process information has been scanned yet, don't update the view
    if let Some(process_list) = process_viewer.process_info() {
        process_store.clear();
        for process in &process_list {
            process_store.insert_with_values(None, &[0, 1, 2],
                                             &[&(process.pid as u64),
                                                 &format!("{}", process.comm),
                                                 &format!("{:?}", process.state)]);
        }
    }
}


pub fn build_ui(application: &gtk::Application) {
    let glade_src = include_str!("gtk/gui_model.glade");
    let builder = gtk::Builder::new_from_string(glade_src);

    let window: gtk::ApplicationWindow = builder.get_object("mainWindow")
        .expect("Couldn't get window.");

    window.set_application(application);

    let process_view: gtk::TreeView = builder.get_object("ProcessTable")
        .expect("Couldn't get list view.");

    let process_store = gtk::ListStore::new(&[
        u64::static_type(),     // PID
        String::static_type(),  // name
        String::static_type(),  // state
    ]);

    process_view.set_model(Some(&process_store));

    // start the background thread that queries /proc continuously
    let proc_scanner = ProcScanner::new();

    // query the process scanning for the first time to initialize the view model
    update_process_view(&process_store, &proc_scanner);

    // query the process scanning thread for the process info every 2 seconds
    gtk::timeout_add(2000, move || {
        update_process_view(&process_store, &proc_scanner);

        glib::Continue(true)
    });

    window.show_all();
}

fn main() {
    let application = gtk::Application::new("com.github.grid",
                                            gio::ApplicationFlags::empty())
                                            .expect("Initialization failed..");

    application.connect_startup(move |app| {
        build_ui(app);
    });

    application.connect_activate(|_| {});

    application.run(&args().collect::<Vec<_>>());

//    let args: Vec<String> = env::args().collect();
//    let target_pid = args[1].parse::<u64>().unwrap();

//    let process_list = proc_io::get_process_info();
//    let process = proc_utils::io::get_pid_info(target_pid);
//    println!("{:?}", process);

//    for process in process_list { println!("{:?}", process); }
}
