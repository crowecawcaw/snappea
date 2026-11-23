//! The `snappea` command line ([`run`]). It runs the commands of [`crate::driver`] on a
//! [`Host`](crate::host::Host), natively [`NativeHost`](crate::host::NativeHost);
//! `snappea-compat`, upstream's `p`, is in [`crate::compat`].

mod command;
mod guide;
mod observe;

pub use command::run;
