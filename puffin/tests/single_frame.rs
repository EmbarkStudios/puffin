mod common;

use std::sync::Arc;

use puffin::{FrameData, GlobalProfiler};

#[test]
fn single_frame() {
    fn profiler_sink(frame_data: Arc<FrameData>) {
        let frame_meta = frame_data.meta();
        assert_eq!(frame_meta.frame_index, 0);
        assert_eq!(frame_meta.num_scopes, 4);
    }

    // Init profiler sink and enable capture
    let sink_id = GlobalProfiler::lock().add_sink(Box::new(profiler_sink));
    puffin::set_scopes_on(true);

    // Run process
    common::process_1();

    // End frame, and uninit profiler
    puffin::GlobalProfiler::lock().new_frame();
    GlobalProfiler::lock().remove_sink(sink_id);
}
