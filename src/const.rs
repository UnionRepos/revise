//! Constants shared with the Python implementation of Revise.

pub const VERSION: &str = "0.1.0";

pub const KEY_MAPPING_CTRL_N: u8 = 0x0e;
pub const KEY_MAPPING_CTRL_C: u8 = 0x03;
pub const KEY_MAPPING_CTRL_P: u8 = 0x10;

/// Sentinel stored in the default `rules` list. A rule is enabled when it is
/// named explicitly, or when it is enabled by default and this sentinel is present.
pub const ALL_ENABLED_NAME: &str = "All rules enabled";

pub const DEFAULT_PRIORITY: i64 = 1000;

pub const ARGUMENT_PLACEHOLDER: &str = "REVISE_ARGUMENT_PLACEHOLDER";

pub const CONFIGURATION_TIMEOUT: f64 = 60.0;

/// Ten zero-width spaces. Instant mode hides them in the prompt and the log
/// reader uses them to find command boundaries.
pub const USER_COMMAND_MARK: &str = "\u{200B}\u{200B}\u{200B}\u{200B}\u{200B}\u{200B}\u{200B}\u{200B}\u{200B}\u{200B}";

pub const LOG_SIZE_IN_BYTES: usize = 1024 * 1024;
pub const LOG_SIZE_TO_CLEAN: usize = 10 * 1024;

pub const DIFF_WITH_ALIAS: f64 = 0.5;

pub const SHELL_LOGGER_SOCKET_ENV: &str = "SHELL_LOGGER_SOCKET";
pub const SHELL_LOGGER_LIMIT: usize = 5;

pub const SETTINGS_HEADER: &str = "\
# Revise settings file
#
# The rules are defined as in the example bellow:
#
# rules = ['cd_parent', 'git_push', 'python_command', 'sudo']
#
# The default values are as follows. Uncomment and change to fit your needs.
# See https://github.com/nvbn/revise#settings for more information.
#
";

/// Commented defaults, matching `repr` of the Python `DEFAULT_SETTINGS` dict.
pub const SETTINGS_DEFAULTS_COMMENTED: &str = "\
# rules = [<const: All rules enabled>]
# exclude_rules = []
# wait_command = 3
# require_confirmation = True
# no_colors = False
# debug = False
# priority = {}
# history_limit = None
# alter_history = True
# wait_slow_command = 15
# slow_commands = ['lein', 'react-native', 'gradle', './gradlew', 'vagrant']
# repeat = False
# instant_mode = False
# num_close_matches = 3
# env = {'LC_ALL': 'C', 'LANG': 'C', 'GIT_TRACE': '1'}
# excluded_search_path_prefixes = []
";

pub const SLOW_COMMANDS: &[&str] = &["lein", "react-native", "gradle", "./gradlew", "vagrant"];

pub const REVISE_ENTRY_POINTS: &[&str] = &["revise"];
