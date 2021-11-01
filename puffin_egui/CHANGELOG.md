# egui_puffin changelog

All notable changes to the egui crate will be documented in this file.


## Unreleased
* Show date-time of when a frame was recorded.


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
