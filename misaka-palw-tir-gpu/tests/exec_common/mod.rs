//! The CPU executor's own random-program generator (`misaka-palw-tir-exec/tests/common/`: the
//! independent second implementation's generator, ported onto `misaka-palw-tir`), included by path
//! so that the device is tested on exactly the programs the CPU executor is — read, never copied.

#![allow(dead_code, clippy::all)]

#[path = "../../../misaka-palw-tir-exec/tests/common/typing.rs"]
pub mod typing;

#[path = "../../../misaka-palw-tir-exec/tests/common/progen.rs"]
pub mod progen;
