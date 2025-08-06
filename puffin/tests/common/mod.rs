#![allow(dead_code)]

use std::{
    io::Write,
    sync::Arc,
    thread::{self, JoinHandle},
    time::Duration,
};

use parking_lot::Mutex;
#[cfg(feature = "serialization")]
use puffin::FrameData;
use puffin::{FrameSinkId, GlobalProfiler};

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

pub struct FrameWriterImpl<W: Write> {
    writer: Arc<Mutex<W>>,
}

impl<W: Write> FrameWriterImpl<W> {
    pub fn from_writer(mut writer: W) -> Self {
        writer.write_all(b"PUF0").unwrap(); //Hack: should not be duplicated
        Self {
            writer: Arc::new(Mutex::new(writer)),
        }
    }

    #[cfg(feature = "serialization")]
    fn write_frame(&self, frame_data: Arc<FrameData>) {
        use std::ops::DerefMut;

        let mut writer = self.writer.lock();
        frame_data.write_into(None, writer.deref_mut()).unwrap();
    }
}

pub struct FrameWriterSink {
    sink_id: FrameSinkId,
    write_thread: Option<JoinHandle<()>>,
}
impl Drop for FrameWriterSink {
    fn drop(&mut self) {
        GlobalProfiler::lock().remove_sink(self.sink_id);
        if let Some(write_handle) = self.write_thread.take() {
            let _ = write_handle.join();
        }
    }
}

#[cfg(feature = "serialization")]
#[must_use]
pub fn init_frames_writer(writer: impl Write + Send + 'static) -> FrameWriterSink {
    use std::sync::mpsc;

    let frame_writer = FrameWriterImpl::from_writer(writer);
    let (frame_sender, frames_recv) = mpsc::channel();

    let write_thread = thread::Builder::new()
        .name("frame_writer".into())
        .spawn(move || {
            while let Ok(frame_data) = frames_recv.recv() {
                frame_writer.write_frame(frame_data);
            }
        })
        .unwrap();

    // Init profiler sink and enable capture
    let sink_id = GlobalProfiler::lock().add_sink(Box::new(move |frame_data| {
        frame_sender.send(frame_data).unwrap()
    }));
    FrameWriterSink {
        sink_id,
        write_thread: Some(write_thread),
    }
}
