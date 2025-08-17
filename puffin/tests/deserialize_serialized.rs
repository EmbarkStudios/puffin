#![cfg(target_os = "linux")]

use std::{
    io::{Seek, Write as _},
    ops::DerefMut,
    sync::Arc,
    thread,
    time::Duration,
};

use memfile::MemFile;
use parking_lot::Mutex;
use puffin::{FrameView, GlobalProfiler, profile_scope, set_scopes_on};

fn run_write(mut file: MemFile) {
    //Hack to get magix needed by `FrameView::read`
    file.write_all(b"PUF0").unwrap();

    // Init profiler sink with sync wrinting
    let writer = Arc::new(Mutex::new(file));
    let sink = GlobalProfiler::lock().add_sink(Box::new(move |frame_data| {
        let mut writer = writer.lock();
        frame_data.write_into(writer.deref_mut()).unwrap();
    }));

    set_scopes_on(true); // need this to enable capture
    // run frames
    for idx in 0..4 {
        profile_scope!("main", idx.to_string());
        {
            profile_scope!("sleep 1ms");
            let sleep_duration = Duration::from_millis(1);
            thread::sleep(sleep_duration);
        }
        {
            profile_scope!("sleep 2ms");
            let sleep_duration = Duration::from_millis(2);
            thread::sleep(sleep_duration);
        }
        GlobalProfiler::lock().new_frame();
    }

    set_scopes_on(false);
    GlobalProfiler::lock().new_frame(); //Force to get last frame
    GlobalProfiler::lock().remove_sink(sink);
}

fn run_read(mut file: MemFile) {
    file.rewind().unwrap();
    let _ = FrameView::read(&mut file).expect("read :");
}

#[test]
fn deserialize_serialized() {
    let file = MemFile::create_default("deserialize_serialized.puffin").unwrap();
    run_write(file.try_clone().unwrap());
    thread::sleep(Duration::from_secs(1));
    run_read(file);
}
