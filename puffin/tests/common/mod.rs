use std::{thread, time::Duration};

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
