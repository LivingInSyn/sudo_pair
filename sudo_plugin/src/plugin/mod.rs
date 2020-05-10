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

//! Utilities for wrapping sudo plugins and the values they're
//! configured with.

mod option_map;
mod command_info;
mod settings;
mod user_info;
mod print_facility;
mod conv_facility;
mod traits;

use super::errors::*;
use super::version::Version;

pub use self::option_map::OptionMap;
pub use self::print_facility::PrintFacility;
pub use self::conv_facility::ConversationFacility;

use self::command_info::CommandInfo;
use self::settings::Settings;
use self::user_info::UserInfo;

use std::convert::TryInto;
use std::collections::HashSet;
use std::path::PathBuf;
use std::ffi::{CString, CStr};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::slice;

use libc::{c_char, c_int, c_uint, gid_t};

/// An implementation of a sudo plugin, initialized and parsed from the
/// values passed to the underlying `open` callback.
#[allow(missing_debug_implementations)]
pub struct Plugin {
    /// The name of the plugin. This will be the generally be the same
    /// as the name of the exported C struct.
    pub plugin_name: String,

    /// The version of the plugin.
    pub plugin_version: Option<String>,

    /// The plugin API version supported by the invoked `sudo` command.
    pub version: Version,

    /// The command being executed, in the same form as would be passed
    /// to the `execve(2)` system call.
    pub command: Vec<CString>,

    /// A map of user-supplied sudo settings. These settings correspond
    /// to flags the user specified when running sudo. As such, they
    /// will only be present when the corresponding flag has been specified
    /// on the command line.
    pub settings: Settings,

    /// A map of information about the user running the command.
    pub user_info: UserInfo,

    /// A map of information about the command being run.
    pub command_info: CommandInfo,

    /// A map of the user's environment variables.
    pub user_env: OptionMap,

    /// A map of options provided to the plugin after the its path in
    /// sudo.conf.
    ///
    /// Settings that aren't of the form `key=value` will have a key
    /// in the map whose value is the same as the key, similar to how
    /// HTML handles valueless attributes (e.g., `disabled` will become
    /// `plugin_options["disabled"] => "disabled"`).
    pub plugin_options: OptionMap,

    stdout: PrintFacility,
    stderr: PrintFacility,

    conversation_f: ConversationFacility,

    _conversation: crate::sys::sudo_conv_t,
}

impl Plugin {
    /// Initializes a `Plugin` from the arguments provided to the
    /// underlying C `open` callback function. Verifies the API version
    /// advertised by the underlying `sudo` is supported by this library,
    /// parses all provided options, and wires up communication
    /// facilities.
    ///
    /// Returns an error if there was a problem initializing the plugin.
    #[cfg_attr(feature="cargo-clippy", allow(clippy::new_ret_no_self))]
    #[cfg_attr(feature="cargo-clippy", allow(clippy::cast_sign_loss))]
    #[cfg_attr(feature="cargo-clippy", allow(clippy::too_many_arguments))]
    #[cfg_attr(feature="cargo-clippy", allow(clippy::missing_safety_doc))]
    pub unsafe fn new(
        plugin_name:    String,
        plugin_version: Option<String>,
        version:        c_uint,
        argc:           c_int,
        argv:           *const *mut c_char,
        settings:       *const *mut c_char,
        user_info:      *const *mut c_char,
        command_info:   *const *mut c_char,
        user_env:       *const *mut c_char,
        plugin_options: *const *mut c_char,
        stdout:         PrintFacility,
        stderr:         PrintFacility,
        conversation:   crate::sys::sudo_conv_t,
        conversation_f: ConversationFacility,
    ) -> Result<Self> {
        let version = Version::from(version).check()?;

        // parse the argv into the command being run
        let mut argv = slice::from_raw_parts(
            argv,
            argc as usize
        ).to_vec();

        let command = argv
            .iter_mut()
            .map(|ptr| CStr::from_ptr(*ptr).to_owned())
            .collect();

        let plugin = Self {
            plugin_name,
            plugin_version,

            version,
            command,

            settings:       OptionMap::from_raw(settings as _).try_into()?,
            user_info:      OptionMap::from_raw(user_info as _).try_into()?,
            command_info:   OptionMap::from_raw(command_info as _).try_into()?,
            user_env:       OptionMap::from_raw(user_env as _),
            plugin_options: OptionMap::from_raw(plugin_options as _),

            stdout,
            stderr,

            _conversation: conversation,
            conversation_f
        };

        Ok(plugin)
    }

    ///
    /// Returns a facility implementing `std::io::Write` that emits to
    /// the invoking user's STDOUT.
    ///
    pub fn stdout(&self) -> PrintFacility {
        self.stdout.clone()
    }

    ///
    /// Returns a facility implementing `std::io::Write` that emits to
    /// the invoking user's STDERR.
    ///
    pub fn stderr(&self) -> PrintFacility {
        self.stderr.clone()
    }

    ///
    /// Returns a facility implementing `std::io::Write` that emits to
    /// the invoking user's STDERR.
    ///
    pub fn conversation(&self) -> ConversationFacility {
        self.conversation_f.clone()
    }

    ///
    /// Returns a facility implementing `std::io::Write` that emits to
    /// the user's TTY, if sudo detected one.
    ///
    pub fn tty(&self) -> Option<Tty> {
        self.user_info.tty.as_ref().and_then(|path|
            Tty::try_from(path).ok()
        )
    }

    ///
    /// As best as can be reconstructed, what was actually typed at the
    /// shell in order to launch this invocation of sudo.
    ///
    // TODO: I don't really like this name
    pub fn invocation(&self) -> Vec<u8> {
        let mut sudo    = self.settings.progname.as_bytes().to_vec();
        let     flags   = self.settings.flags();

        if !flags.is_empty() {
            sudo.push(b' ');
            sudo.extend_from_slice(&flags.join(&b' ')[..]);
        }

        for entry in &self.command {
            sudo.push(b' ');
            sudo.extend_from_slice(entry.as_bytes());
        }

        sudo
    }

    ///
    /// The `cwd` to be used for the command being run. This is
    /// typically set on the `user_info` component, but may be
    /// overridden by the policy plugin setting its value on
    /// `command_info`.
    ///
    pub fn cwd(&self) -> &PathBuf {
        self.command_info.cwd.as_ref().unwrap_or(
            &self.user_info.cwd
        )
    }

    ///
    /// The complete set of groups the invoked command will have
    /// privileges for. If the `-P` (`--preserve-groups`) flag was
    /// passed to `sudo`, the underlying `command_info` will not have
    /// this set and this method will return the list of original groups
    /// from the running the command.
    ///
    /// This set will always contain `runas_egid`.
    ///
    pub fn runas_gids(&self) -> HashSet<gid_t> {
        // sanity-check that if preserve_groups is unset we have
        // `runas_groups`, and if it is set that we don't
        if self.command_info.preserve_groups {
            debug_assert!(self.command_info.runas_groups.is_none())
        } else {
            debug_assert!(self.command_info.runas_groups.is_some())
        }

        // even though the above sanity-check might go wrong, it still
        // seems like a safe bet that if `runas_groups` isn't set that
        // the command will be invoked with the original user's groups
        // (it will probably require reading the `sudo` source code to
        // verify this)
        let mut set : HashSet<_> = self.command_info.runas_groups.as_ref().unwrap_or(
            &self.user_info.groups
        ).iter().cloned().collect();

        // `command_info.runas_egid` won't necessarily be in the list of
        // `command_info.runas_groups` if `-P` was passed; however, the
        // user will have this in the list of groups that they will gain
        // permissions for so it seems sane to include it in this list
        let _ = set.insert(self.command_info.runas_egid);

        set
    }
}

///
/// A facility implementing `std::io::Write` that allows printing
/// output to directly to the terminal of the user invoking `sudo`.
///
#[derive(Debug)]
pub struct Tty(File);

impl Tty {
    fn try_from(path: &Path) -> io::Result<Self> {
        OpenOptions::new().write(true).open(path).map(Tty)
    }
}

impl Write for Tty {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
