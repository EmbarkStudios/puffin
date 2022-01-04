# egui_puffin changelog

All notable changes to the egui crate will be documented in this file.


## Unreleased
* Fix compilation for `wasm32-unknown-unknown`.


## 0.11.0 - 2021-11-16
* Show total frames recorded and their total size.
* In-memory compression of frames to use up less RAM.
* Add slider to control how many recent frames to store.


## 0.10.3 - 2021-11-08
* Fix vertical scrolling in flamgraph.
* Show thread names in stats tab.


## 0.10.2 - 2021-11-05
* Normalize frame height based on what frames are visible.


## 0.10.1 - 2021-11-02
* Show scrollbar for history of recent frames.
* Show date-time of when a frame was recorded.
* Show compressed size of selected frame.
* Fix occasional flickering when viewing merged scopes.
* Handle gaps in incoming frames.


## 0.10.0 - 2021-10-29
* Update to egui 0.15.


## 0.9.1 - 2021-10-21
* Add a scope filter to focus on certain scopes.


## 0.9.0 - 2021-10-12
* You can now select multiple frames.


## 0.8.0 - 2021-09-20
* `ProfilerUi` now takes by argument the profiling data to view. You may want to use `GlobalProfilerUi` instead.


## 0.7.0 - 2021-09-06
* Add a stats panel for finding high-bandwidth scopes.


## 0.6.0 - 2021-08-25
* Update to egui 0.14


## 0.5.0 - 2021-08-23
* Show frame index.


## 0.4.0 - 2021-07-05
* Update to egui 0.13
* Paint flamegraph top-down
* Scrollable flamegraph
* Option to sort threads by name
* Drag with right mouse button to zoom
* Toggle play/pause with spacebar
* More compact UI
* Show all scopes (even tiny ones)


## 0.3.0 - 2021-05-27
* History viewer.
* Update to puffin 0.5.1.


## 0.2.0 - 2021-05-13
* Update to egui 0.12.
* Remove drag-to-zoom (scroll to zoom instead).


## 0.1.0 - 2021-05-05 - Initial release
Show flamegraph plot of either latest frame, a spike frame, or a paused frame.
The view supports viewing merged sibling scopes.
