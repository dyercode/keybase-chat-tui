#!/usr/bin/env sh
# -Dwarnings  to make warnings fail
cargo clippy --  -Dclippy::style -Wclippy::restriction -Aclippy::arithmetic_side_effects -Aclippy::integer_arithmetic -Aclippy::implicit_return -Aclippy::missing_docs_in_private_items -Aclippy::default_numeric_fallback -Aclippy::single_char_lifetime_names -Aclippy::missing_docs_in_private_items
