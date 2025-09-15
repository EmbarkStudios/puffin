#![allow(dead_code)]

use std::{
    thread::{self},
    time::Duration,
};

pub fn process_1() {
    puffin::profile_function!();
    sub_process_1_1();
    (0..2).for_each(|_| sub_process_1_2());
}

fn sub_process_1_1() {
    puffin::profile_function!();
    thread::sleep(Duration::from_millis(1));
}

fn sub_process_1_2() {
    puffin::profile_function!();
    thread::sleep(Duration::from_micros(2));
}

pub fn example_run() {
    for idx in 0..4 {
        puffin::profile_scope!("main", idx.to_string());

        {
            puffin::profile_scope!("sleep 1ms");
            let sleep_duration = Duration::from_millis(1);
            thread::sleep(sleep_duration);
        }

        {
            puffin::profile_scope!("sleep 2ms");
            let sleep_duration = Duration::from_millis(2);
            thread::sleep(sleep_duration);
        }
        //println!("before new_frame {idx}");
        puffin::GlobalProfiler::lock().new_frame();
        //println!("after new_frame {idx}");
    }
}
