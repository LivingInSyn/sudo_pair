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

//! The collection of `Error` types used by this library.

// TODO: use error types as directly defined by sudo_plugin(8).

// TODO: remove when error_chain is updated to compile cleanly
#![allow(bare_trait_objects)]
#![allow(renamed_and_removed_lints)]
#![allow(single_use_lifetimes)]
#![allow(variant_size_differences)]

use crate::version::Version;

use sudo_plugin_sys as sys;
use libc::c_int;
use error_chain::error_chain;

pub use error_chain::bail;

error_chain! {
    errors {
        /// An error which can be returned when the requsested plugin API
        /// version is incompatible with the version implemented by this
        /// library.
        UnsupportedApiVersion(cur: Version) {
            description("sudo doesn't support the minimum plugin API version required by this plugin"),
            display("sudo called this plugin with an API version of {}, but a minimum of {} is required", cur, Version::minimum()),
        }

        /// An error which can be returned when there's a general error
        /// when initiailizing the plugin.
        Uninitialized {
            description("the plugin failed to initialize"),
            display("the plugin failed to initialize"),
        }

        /// An error which can be returned if the user is not authorized
        /// to invoke sudo with the provided command and/or options.
        Unauthorized {
            description("command unauthorized"),
            display("command unauthorized"),
        }
    }
}

/// A trait that is implemented by all Error types in this library, which
/// allows any error to be converted to its corresponding integer error
/// code as understood by the sudo plugin API.
///
/// The sudo plugin API understands the following error codes:
///
/// *  1: Success
/// *  0: Failure
/// * -1: General error
/// * -2: Usage error
pub trait AsSudoPluginRetval {
    /// Converts the error to its corresponding integer error code for
    /// the I/O plugin `open` function.
    fn as_sudo_io_plugin_open_retval(&self) -> c_int;

    /// Converts the error to its corresponding integer error code for
    /// the I/O plugin `log_*` suite of functions.
    fn as_sudo_io_plugin_log_retval(&self) -> c_int;
}

impl<T, E: AsSudoPluginRetval> AsSudoPluginRetval for ::std::result::Result<T, E> {
    fn as_sudo_io_plugin_open_retval(&self) -> c_int {
        match *self {
            Ok(_)      => sys::SUDO_PLUGIN_OPEN_SUCCESS,
            Err(ref e) => e.as_sudo_io_plugin_open_retval(),
        }
    }

    fn as_sudo_io_plugin_log_retval(&self) -> c_int {
        match *self {
            Ok(_)      => sys::SUDO_PLUGIN_LOG_OK,
            Err(ref e) => e.as_sudo_io_plugin_log_retval(),
        }
    }
}

impl AsSudoPluginRetval for Error {
    fn as_sudo_io_plugin_open_retval(&self) -> c_int {
        match *self {
            Error(ErrorKind::Unauthorized, _) => sys::SUDO_PLUGIN_OPEN_GENERAL_ERROR,
            Error(_, _)                       => sys::SUDO_PLUGIN_OPEN_FAILURE,
        }
    }

    fn as_sudo_io_plugin_log_retval(&self) -> c_int {
        match *self {
            Error(ErrorKind::Unauthorized, _) => sys::SUDO_PLUGIN_LOG_REJECT,
            Error(_, _)                       => sys::SUDO_PLUGIN_LOG_ERROR,
        }
    }
}
