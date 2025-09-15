mod common;

#[cfg(feature = "serialization")]
#[test]
fn frame_serialization() {
    use puffin::{FramesWriter, SinkManager};

    let frame_data = Vec::new();
    let _frame_writer = FramesWriter::from_writer(frame_data, SinkManager::default()).unwrap();

    puffin::set_scopes_on(true); // enable capture

    common::example_run();

    puffin::set_scopes_on(false);
    puffin::GlobalProfiler::lock().new_frame(); //Force to get last frame
}
