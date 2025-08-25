#![cfg(all(feature = "serialization", not(target_arch = "wasm32")))] // FrameData.write_into not available on wasm

use crate::{FrameData, FrameSinkId, SinkManager};
use anyhow::Context;
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
};

/// Write [`FrameData`] from profiler in a file (or other object than impl [`Write`])
///
/// This register as sink on profiler([`GlobalProfiler`]) and create a thread to write the [`FrameData`].
/// This can be useful If you want to capture and backup the profiling without use puffin_viewer.
///
/// [`GlobalProfiler`]: struct.GlobalProfiler.html
pub struct FramesWriter {
    sink_id: FrameSinkId,
    write_thread: Option<JoinHandle<()>>,
    sink_mngr: SinkManager,
}

impl FramesWriter {
    /// Creates a file from "path" and create [`FramesWriter`] to writes the profiling result to it.
    ///
    /// Errors
    /// Will return the `std::io::Error` if the file creation failed.
    /// Will return the error from `FramesWriter::from_writer` if fail.
    ///
    /// Usage:
    ///
    /// ``` no_run
    /// fn main() {
    ///     let _frame_writer = puffin::FramesWriter::from_path("capture.puffin", puffin::SinkManager::default());
    ///
    ///     puffin::set_scopes_on(true); // you may want to control this with a flag
    ///     // game loop
    ///     loop {
    ///         puffin::GlobalProfiler::lock().new_frame();
    ///         {
    ///             puffin::profile_scope!("slow_code");
    ///             slow_code();
    ///         }
    ///     }
    /// }
    ///
    /// # fn slow_code(){}
    /// ```
    pub fn from_path(
        path: impl AsRef<Path>,
        sink_mngr: SinkManager,
    ) -> Result<FramesWriter, anyhow::Error> {
        let file_writer = BufWriter::new(File::create(path)?);
        Self::from_writer(file_writer, sink_mngr)
    }

    /// Create [`FramesWriter`] to writes the profiling result to the writer.
    ///
    /// Errors
    /// Will return the error from `FramesWriter` creation if fail.
    /// Will return the error from thread creation.
    ///
    /// Usage:
    ///
    /// ``` no_run
    /// use std::net::TcpStream;
    ///
    /// fn main() {
    ///     let mut stream = TcpStream::connect("127.0.0.1:34254").unwrap();
    ///     let _frame_writer = puffin::FramesWriter::from_writer(stream, puffin::SinkManager::default());
    ///
    ///     puffin::set_scopes_on(true); // you may want to control this with a flag
    ///     // game loop
    ///     loop {
    ///         puffin::GlobalProfiler::lock().new_frame();
    ///         {
    ///             puffin::profile_scope!("slow_code");
    ///             slow_code();
    ///         }
    ///     }
    /// }
    ///
    /// # fn slow_code(){}
    /// ```
    pub fn from_writer(
        writer: impl Write + Send + 'static,
        sink_mngr: SinkManager,
    ) -> Result<Self, anyhow::Error> {
        let (frame_sender, frames_recv) = mpsc::channel();
        let frame_writer =
            FrameWriterImpl::from_writer(writer, frames_recv).context("create FrameWriter")?;

        let write_thread = thread::Builder::new()
            .name("frame_writer".into())
            .spawn(move || frame_writer.run())?;

        // Init profiler sink and enable capture
        let sink_id = sink_mngr.add_sink(Box::new(move |frame_data| {
            frame_sender.send(frame_data).unwrap()
        }));
        Ok(Self {
            sink_id,
            write_thread: Some(write_thread),
            sink_mngr,
        })
    }
}

impl Drop for FramesWriter {
    fn drop(&mut self) {
        self.sink_mngr.remove_sink(self.sink_id);

        // Wait the end of the write to avoid data lost
        if let Some(write_handle) = self.write_thread.take() {
            let _ = write_handle.join();
        }
    }
}

// handle the writing thread.
struct FrameWriterImpl<W: Write> {
    writer: W,
    recv: Receiver<Arc<FrameData>>,
}

impl<W: Write> FrameWriterImpl<W> {
    fn from_writer(mut writer: W, recv: Receiver<Arc<FrameData>>) -> Result<Self, anyhow::Error> {
        writer
            .write_all(b"PUF0") //HACK: value b"PUF0" should not be duplicated
            .context("Write puffin magic file marker")?;
        Ok(Self { writer, recv })
    }

    fn run(mut self) {
        while let Ok(frame_data) = self.recv.recv() {
            frame_data.write_into(&mut self.writer).expect(
                "write frame data shouldn't failed, unless problem with write (not handled)",
            );
            // Flush to avoid lost data if application is closed unexpectedly (like with a crash)
            self.writer
                .flush()
                .expect("writer defaults are not handled")
        }
    }
}
