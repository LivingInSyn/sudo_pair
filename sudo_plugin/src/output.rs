// Copyright 2018 Square Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
// implied. See the License for the specific language governing
// permissions and limitations under the License.

//! A module that includes input and output modules for
//! a sudo plugin

pub(crate) mod print_facility;
pub(crate) mod tty;
pub mod conv_facility;

pub(crate) use print_facility::PrintFacility;
pub(crate) use tty::Tty;
pub(crate) use conv_facility::*;

use crate::sys;

#[derive(Clone, Copy, Debug)]
#[repr(u32)]
enum Level {
    Info  = sys::SUDO_CONV_INFO_MSG,
    Error = sys::SUDO_CONV_ERROR_MSG,
}
