# Changelog

All notable changes to `puffin_http` will be documented in this file.


## Unreleased


## 0.9.0 - 2021-11-16
* In-memory compression of frames to use up less RAM.


## 0.8.0 - 2021-11-12
* Update to puffin 0.11.0.


## 0.7.3 - 2021-11-08
* Add `Server::num_clients`.


## 0.7.2 - 2021-10-28
* Send all outstanding frames on shutdown.


## 0.7.0 - 2021-10-12
* Nothing new.


## 0.6.0 - 2021-09-20
* Better handle slow clients, especially when there are multiple clients.


## 0.5.1 - 2021-09-16
* Fix high-bandwidth connection interruptions.


## 0.5.0 - 2021-09-06
* Switch from lz4 to zstd compression for 50% file size and bandwidth reduction.


## 0.4.1 - 2021-08-24
* Do less work when no clients are connected.


## 0.4.0 - 2021-08-23
* Remove `Server::update` (no longer needed).
* Compress the TCP stream (approximately 75% bandwidth reduction).


## 0.3.0
* Initial release
