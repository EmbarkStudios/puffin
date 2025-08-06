mod common;

#[cfg(feature = "serialization")]
#[test]
fn frame_serialization() {
    let frame_data = Vec::new();
    let _frame_writer = common::init_frames_writer(frame_data);

    //println!("set_scopes_on(true)");
    puffin::set_scopes_on(true); // need this to enable capture

    common::example_run();

    //println!("set_scopes_on(false)");
    puffin::set_scopes_on(false);
    puffin::GlobalProfiler::lock().new_frame(); //Force to get last frame
}
