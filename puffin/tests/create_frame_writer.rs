use parking_lot::RwLock;
use puffin::{FramesWriter, SinkManager};
use std::{io::Write, sync::Arc, thread, time::Duration};

#[derive(Debug, Clone)]
struct FrameContent(Arc<RwLock<Vec<u8>>>);
impl Write for FrameContent {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.write().flush()
    }
}

#[test]
fn create_frame_writer() {
    let frame_data = FrameContent(Arc::new(RwLock::new(Vec::new())));

    let _frame_writer =
        FramesWriter::from_writer(frame_data.clone(), SinkManager::default()).unwrap();
    puffin::set_scopes_on(true);

    {
        puffin::profile_scope!("main");
        thread::sleep(Duration::from_millis(1));
    }
    puffin::GlobalProfiler::lock().new_frame();
    assert!(!frame_data.0.read().is_empty());
}

//TODO: wait some frames before create the FrameWriter
// and check than no scopes are missing
